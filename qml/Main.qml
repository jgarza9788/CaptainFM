import QtQuick
import QtQuick.Window
import QtQuick.Layouts
import CaptainFM

Window {
    id: app
    width: 1180
    height: 720
    visible: true
    flags: Qt.Window | Qt.FramelessWindowHint
    title: pickMode ? (pickReq.title ?? "Choose") : (pane ? "CaptainFM — " + pane.pretty(pane.dir.path) : "CaptainFM")
    color: Style.bgDarker

    // ---- state --------------------------------------------------------------------------

    property int tabIndex: 0
    property var tabPages: []
    readonly property Item tab: tabPages.length > tabIndex ? tabPages[tabIndex] : null
    readonly property Item pane: tab ? tab.activePane : null

    property bool inspectorOn: true
    property bool showHidden: false
    property string groupBy: "none"   // none | type | date | size
    property string sortKey: "name"
    property bool sortDesc: false
    property var yankPaths: []
    property bool yankCut: false
    property var userKeys: ({})
    property bool ready: false
    property string keymap: "fp"     // fp | vim
    property string sidebarMode: "open"   // open | icons | hidden
    readonly property var sidebarModes: ["open", "icons", "hidden"]

    function cycleSidebar() {
        sidebarMode = sidebarModes[(sidebarModes.indexOf(sidebarMode) + 1) % sidebarModes.length]
        toast(sidebarMode === "open" ? "Sidebar open" : (sidebarMode === "icons" ? "Sidebar: icons only" : "Sidebar hidden"))
    }
    property string indexText: ""
    property var aliases: ({})      // alias text -> command id
    property var pinned: []         // command ids shown first in the context menu
    property bool pickMode: false
    property var pickReq: ({})

    readonly property var commands: commandsRef
    readonly property alias prompt: promptBar
    readonly property alias jump: jumpBar
    readonly property alias picker: pickerBox
    // Big dialogs are built the first time they are opened, which keeps them out of startup.
    function lazyBatch() { batchLoader.active = true; return batchLoader.item.dlg }
    function lazySettings() { settingsLoader.active = true; return settingsLoader.item.dlg }
    function lazyConnect() { connectLoader.active = true; return connectLoader.item.dlg }
    readonly property var batchRename: ({ open: (...args) => lazyBatch().open(...args) })
    function openSettings() { lazySettings().open() }
    readonly property alias transfers: transferBox
    readonly property bool overlayOpen: promptBar.visible || jumpBar.visible || pickerBox.visible || (batchLoader.item !== null && batchLoader.item.dlg.visible) || (settingsLoader.item !== null && settingsLoader.item.dlg.visible) || (connectLoader.item !== null && connectLoader.item.dlg.visible) || clashBox.visible

    ListModel { id: tabsModel }

    // ---- helpers ------------------------------------------------------------------------

    function toast(message, kind) { toastBox.show(message, kind ?? "info") }
    property double overlayClosedAt: 0
    function refocus() { overlayClosedAt = Date.now(); keyRoot.forceActiveFocus() }
    readonly property alias inspector: inspectorPane

    // "Ctrl+M play · Ctrl+Left back 10 s · ..." for the player's hotkey note (follows rebinding).
    function mediaHotkeys() {
        const part = (id, label) => {
            const k = engine.keysFor(id).map(engine.prettySeq)[0]
            return k ? "<font color=\"" + Style.yellow + "\">" + k.replace(/</g, "&lt;") + "</font> " + label : ""
        }
        return [part("media.toggle", "play/pause"), part("media.seekBack", "−10 s"), part("media.seekFwd", "+10 s"),
                part("media.mute", "mute"), part("media.loop", "loop")].filter(x => x !== "").join(" · ")
    }
    function pinsChanged() { sidebar.refreshPinned() }

    function isPinned(path) {
        try { return JSON.parse(Backend.bookmarks()).indexOf(path) >= 0 } catch (e) { return false }
    }

    // Two presses in a row for anything that is hard to undo: the first only marks the target (red in the sidebar and
    // tab strip) and says in a toast what the second will do; the second, within 3 seconds, does it. A right-click twice
    // does this on a pinned entry (unpin), a drive (unmount), a remote (disconnect), a tab (close) and a file or folder
    // (delete, which goes to the trash); Ctrl+D or Ctrl+B twice unpins.
    property string armedKey: ""
    property var armedAction: null
    Timer { id: armTimer; interval: 3000; onTriggered: { app.armedKey = ""; app.armedAction = null } }

    // Returns true when this was the second press and the action ran. `done` is the toast after it (empty: the action reports).
    function armTwice(key, ask, done, action) {
        if (armedKey === key) { runArmed(); return true }
        armedKey = key
        armedAction = () => { action(); if (done) toast(done) }
        armTimer.restart()
        toast(ask, "warn")
        return false
    }
    function runArmed() {
        const act = armedAction
        armTimer.stop()
        armedKey = ""
        armedAction = null
        if (act) act()
    }

    function togglePinned(path, name) {
        if (!isPinned(path)) { Backend.toggleBookmark(path); toast("Pinned " + name); pinsChanged(); return }
        armTwice("pin:" + path, "Again to unpin “" + name + "”", "Unpinned " + name, () => { Backend.toggleBookmark(path); pinsChanged() })
    }

    // Right-click twice on a sidebar drive or remote.
    function deviceRightClicked(item) {
        const d = item.dev
        if (!d) return
        if (d.kind === "disk" && item.path === "/") { toast("The system drive can’t be unmounted", "warn"); return }
        const remote = d.kind !== "disk"
        const verb = remote ? "disconnect" : "unmount"
        armTwice("drive:" + item.path, "Again to " + verb + " “" + item.label + "”", (remote ? "Disconnecting " : "Unmounting ") + item.label + "…",
                 () => Backend.ejectDevice(JSON.stringify(d)))
    }

    // Right-click twice on a tab.
    function tabRightClicked(i, title) {
        if (tabsModel.count <= 1) { toast("That is the last tab (Ctrl+Q quits)", "warn"); return }
        armTwice("tab:" + i, "Again to close tab “" + title + "”", "Closed tab “" + title + "”", () => closeTab(i))
    }

    // A right-click on a file or folder opens its menu, as before, and arms the delete; a second right-click on the same
    // item (or on the menu) within 3 seconds sends it to the trash (undo with Ctrl+Z).
    function rowRightClicked() {
        const p = pane
        if (!p || p.currentPath === "") { contextMenu(); return }
        const many = p.dir.selectedCount > 1
        const paths = many ? JSON.parse(p.dir.selectedPaths()) : [p.currentPath]
        const name = many ? paths.length + " items" : p.baseName(p.currentPath)
        const kind = many ? "" : (p.currentIsDir ? "folder " : "file ")
        const key = "item:" + JSON.stringify(paths)
        if (armedKey === key) { pickerBox.close(); runArmed(); return }
        contextMenu()
        armTwice(key, "Right-click again to delete " + kind + "“" + name + "”", "", () => Backend.trash(JSON.stringify(paths)))
    }

    // The sidebar's global search box drives a whole-drive search in the active pane.
    function sidebarSearch(text) {
        const p = pane
        if (!p) return
        if (text.trim() === "") { if (p.deep) p.exitDeep(); return }
        if (!p.deep) p.dir.deepStart(Backend.mountRoot(p.dir.path))
        p.searchText = text
        p.dir.deepQuery(text)
    }

    // Palette fallback: when no command matches, the user probably means a folder.
    function folderItems(query) {
        const out = []
        const typed = Backend.expandPath(query.trim())
        const seen = {}
        const add = (path, category) => {
            if (seen[path]) return
            seen[path] = true
            out.push({ title: pane ? pane.pretty(path) : path, category: category, hint: "", keys: [], bindable: false, tag: { folder: path } })
        }
        if (typed !== "" && Backend.isDir(typed)) add(typed, "folder")
        try {
            for (const c of JSON.parse(Backend.jump(query))) add(c.path, c.kind === "pinned" ? "pinned" : (c.kind === "recent" ? "recent" : "folder"))
        } catch (e) {}
        return out
    }

    function newTab(path, layout) {
        const paths = Array.isArray(path) ? path : [path]
        const views = Prefs.v.defaultView === "list" ? [] : paths.map(() => Prefs.v.defaultView)
        tabsModel.append({ paths: JSON.stringify(paths), views: JSON.stringify(views), layout: layout ?? "columns" })
        tabIndex = tabsModel.count - 1
        markDirty()
    }

    function selectTab(i) {
        if (tabsModel.count === 0) return
        tabIndex = ((i % tabsModel.count) + tabsModel.count) % tabsModel.count
        markDirty()
    }

    function closeTab(i) {
        if (tabsModel.count <= 1) { quit(); return }
        tabsModel.remove(i)
        tabIndex = Math.min(tabIndex > i ? tabIndex - 1 : tabIndex, tabsModel.count - 1)
        markDirty()
    }

    function snapshot() {
        const tabs = tabPages.map(t => ({
            paths: t.paneItems.map(p => p.dir.path),
            views: t.paneItems.map(p => p.viewMode),
            layout: t.layoutMode,
            active: t.activeIndex
        }))
        return { tabs: tabs, tabIndex: tabIndex }
    }

    function applySnapshot(s) {
        tabsModel.clear()
        for (const t of s.tabs) tabsModel.append({
            paths: JSON.stringify(t.paths), views: JSON.stringify(t.views ?? []), layout: t.layout ?? "columns"
        })
        tabIndex = Math.max(0, Math.min(s.tabIndex ?? 0, tabsModel.count - 1))
        Qt.callLater(() => {
            for (let i = 0; i < tabPages.length && i < s.tabs.length; i++)
                tabPages[i].activeIndex = Math.min(s.tabs[i].active ?? 0, tabPages[i].paneCount - 1)
        })
    }

    function saveSession() {
        if (!ready || pickMode) return
        const s = snapshot()
        s.inspector = inspectorOn
        s.showHidden = showHidden
        s.sortKey = sortKey
        s.sortDesc = sortDesc
        s.zoom = Style.zoomTarget
        s.animations = Style.animations
        s.density = Style.density
        s.pinned = pinned
        s.keymap = keymap
        s.groupBy = groupBy
        s.sidebarMode = sidebarMode
        s.folds = sidebar.folded
        Backend.writeConfig("session", JSON.stringify(s))
    }

    function markDirty() { if (ready) saveTimer.restart() }

    function quit() {
        if (pickMode) { Backend.pickCancel(); return }
        saveSession()
        Qt.quit()
    }

    // ---- file picker mode ---------------------------------------------------------------------

    function pickExtensions() {
        const f = (pickReq.filters ?? [])[pickBar.filterIndex]
        if (!f) return []
        const exts = []
        for (const pat of f.patterns) {
            const m = /^\*\.([A-Za-z0-9_.+-]+)$/.exec(pat)
            if (m) exts.push(m[1].toLowerCase())
            else if (pat === "*") return []      // "all files"
        }
        return exts
    }

    function applyPickFilter() {
        if (!pickMode || !pane) return
        pane.dir.setExtFilter(JSON.stringify(pickExtensions()))
    }

    function pickCycleFilter() {
        const n = (pickReq.filters ?? []).length
        if (n === 0) return
        pickBar.filterIndex = (pickBar.filterIndex + 1) % n
        applyPickFilter()
    }

    function pickFinish(paths) {
        Backend.pickDone(JSON.stringify({ paths: paths }))
    }

    function pickAccept() {
        const p = pane
        if (!p) return
        if (pickReq.save) {
            const name = pickBar.name.trim()
            if (name === "" || name.indexOf("/") >= 0) { toast("Enter a file name", "warn"); pickBar.focusName(); return }
            const target = p.dir.path + "/" + name
            if (Backend.exists(target)) {
                promptBar.open({
                    label: "Replace “" + name + "”?",
                    confirm: true,
                    accept: () => pickFinish([target]),
                    cancel: () => refocus()
                })
                return
            }
            pickFinish([target])
            return
        }
        let sel = JSON.parse(p.dir.selectedPaths())
        if (pickReq.directory) {
            if (sel.length === 0) sel = [p.currentIsDir && p.currentPath !== "" ? p.currentPath : p.dir.path]
            pickFinish(pickReq.multiple ? sel : [sel[0]])
            return
        }
        if (sel.length === 0 && p.currentPath !== "" && !p.currentIsDir) sel = [p.currentPath]
        sel = sel.filter(path => !Backend.isDir(path))
        if (sel.length === 0) { toast("Choose a file", "warn"); return }
        pickFinish(pickReq.multiple ? sel : [sel[0]])
    }

    // Enter on a file: accept in open mode, copy its name in save mode.
    function pickEnterFile() {
        const p = pane
        if (!p || p.currentPath === "") return
        if (pickReq.save) { pickBar.setName(p.baseName(p.currentPath)); return }
        pickAccept()
    }

    function saveLayoutPrompt() {
        promptBar.open({
            label: "Save layout as:",
            text: "",
            accept: (name) => {
                if (name === "") return
                const all = JSON.parse(Backend.readConfig("layouts"))
                all[name] = snapshot()
                Backend.writeConfig("layouts", JSON.stringify(all))
                toast("Saved layout “" + name + "”")
            }
        })
    }

    function loadLayoutPicker() {
        const all = JSON.parse(Backend.readConfig("layouts"))
        const names = Object.keys(all)
        if (names.length === 0) { toast("No saved layouts yet (, l s)", "warn"); return }
        pickerBox.open({
            placeholder: "Load layout",
            items: names.map(n => ({ title: n, hint: all[n].tabs.length + " tab(s)", tag: n })),
            choose: (item) => { applySnapshot(all[item.tag]); refocus() }
        })
    }

    // Devices and remotes ---------------------------------------------------------------------

    property string devicesMode: "open"     // open | eject

    function devicesPicker(mode) {
        devicesMode = mode ?? "open"
        toast("Looking for devices…")
        Backend.listDevices()
    }

    function showDevices(json) {
        let list = JSON.parse(json)
        if (devicesMode === "eject") list = list.filter(d => d.mounted && d.removable)
        if (list.length === 0) { toast(devicesMode === "eject" ? "Nothing to eject" : "No devices found", "warn"); return }
        pickerBox.open({
            placeholder: devicesMode === "eject" ? "Unmount or eject…" : "Open a device or remote",
            items: list.map(d => ({
                title: d.label + (d.kind === "remote" || d.kind === "cloud" ? "" : "  " + d.detail),
                hint: d.mounted ? (d.path || d.uri) : "not mounted",
                tag: d
            })),
            choose: (item) => {
                const d = item.tag
                refocus()
                if (devicesMode === "eject") { Backend.ejectDevice(JSON.stringify(d)); return }
                if (d.mounted) { if (pane) pane.navigate(d.path); return }
                pendingMount = true
                toast("Mounting " + d.label + "…")
                Backend.mountDevice(JSON.stringify(d))
            }
        })
    }

    property bool pendingMount: false

    function handleMount(json) {
        const r = JSON.parse(json)
        if (!r.ok) { pendingMount = false; toast(r.message, "error"); return }
        toast(r.message)
        if (pendingMount && r.path !== "" && pane) pane.navigate(r.path)
        pendingMount = false
    }

    // Saved connections first, then a new connection or a plain URL. `kinds` (service ids) limits the list
    // to one service, e.g. ["sftp"]; without it everything is offered.
    function connectPicker(kinds) {
        const only = kinds && kinds.length > 0
        const label = only ? lazyConnect().kinds.find(k => k.id === kinds[0]).label : ""
        const items = [{
            title: only ? "New " + label + " connection…" : "New connection…",
            hint: only ? "" : "SFTP, SMB, FTP, WebDAV, Google Drive and other clouds",
            tag: { make: "new" }
        }]
        for (const c of JSON.parse(Backend.savedConnections())) {
            if (only && kinds.indexOf(c.kind) < 0) continue
            items.push({ title: c.name, hint: c.kind + (c.host ? "://" + (c.user ? c.user + "@" : "") + c.host : "") + (c.remember ? "  ·  password in keyring" : ""), tag: c })
        }
        if (!only) items.push({ title: "Connect to a URL…", hint: "smb://, sftp://, ftp://, davs://", tag: { make: "url" } })
        pickerBox.open({
            placeholder: only ? "Connect: " + label + "…" : "Connect to…",
            hintRight: "ctrl+e edits · ctrl+d forgets",
            items: items,
            choose: (item) => {
                const c = item.tag
                if (c.make === "new") { lazyConnect().open({}, only ? kinds[0] : undefined); return }
                if (c.make === "url") { connectUrlPrompt(); return }
                refocus()
                pendingMount = true
                toast("Connecting to " + c.name + "…")
                Backend.connectSaved(c.id)
            },
            keyHook: (e, item) => {
                if (!(e.modifiers & Qt.ControlModifier) || item.tag.id === undefined) return false
                if (e.key === Qt.Key_D) {
                    pickerBox.close()
                    Backend.deleteConnection(item.tag.id)
                    return true
                }
                if (e.key === Qt.Key_E) {
                    pickerBox.close()
                    lazyConnect().edit(item.tag)
                    return true
                }
                return false
            }
        })
    }

    // How often each command, preview kind and plugin was used over the last four weeks, least used first.
    function showFeatureUse() {
        const r = JSON.parse(Backend.featureReport())
        if (r.rows.length === 0) { toast("Nothing counted yet", "warn"); return }
        const title = (id) => {
            if (id.indexOf("preview:") === 0) return "Preview: " + id.substring(8)
            if (id.indexOf("plugin:") === 0) return "Plugin: " + id.substring(7)
            const c = commands.byId(id)
            return c ? c.title : id
        }
        const pct = (d) => r.opened > 0 ? Math.round(Math.min(d, r.opened) * 100 / r.opened) : 0
        pickerBox.open({
            placeholder: "Used on " + r.opened + " of the last " + r.window + " days · least used first",
            items: r.rows.slice().reverse().map(x => ({
                title: title(x.id),
                hint: x.days + (x.days === 1 ? " day" : " days") + " (" + pct(x.days) + "%) · " + (x.last === 0 ? "today" : x.last + " days ago"),
                tag: x
            })),
            choose: (item) => refocus()
        })
    }

    // Saved connections: Enter edits (name, host, user, password…), ctrl+d forgets.
    function manageConnections() {
        const saved = JSON.parse(Backend.savedConnections())
        if (saved.length === 0) { toast("No saved connections", "warn"); return }
        pickerBox.open({
            placeholder: "Edit connection…",
            hintRight: "enter edits · ctrl+d forgets",
            items: saved.map(c => ({
                title: c.name,
                hint: c.kind + (c.host ? "://" + (c.user ? c.user + "@" : "") + c.host + (c.path ? "/" + c.path.replace(/^\//, "") : "") : "") + (c.remember ? "  ·  password in keyring" : ""),
                tag: c
            })),
            choose: (item) => lazyConnect().edit(item.tag),
            keyHook: (e, item) => {
                if ((e.modifiers & Qt.ControlModifier) && e.key === Qt.Key_D) {
                    pickerBox.close()
                    Backend.deleteConnection(item.tag.id)
                    return true
                }
                return false
            }
        })
    }

    function connectUrlPrompt() {
        promptBar.open({
            label: "Connect to server:",
            text: "sftp://",
            accept: (url) => {
                if (url === "") return
                pendingMount = true
                toast("Connecting…")
                Backend.connectRemote(url)
            }
        })
    }

    function connectPrompt() { connectPicker() }

    // The edit form was filled in: save it, no connecting.
    function runUpdate(req) { Backend.updateConnection(JSON.stringify(req)) }

    // The form was filled in; sign-in for cloud drives happens in the browser and can take a while.
    function runConnect(req) {
        pendingMount = true
        toast(req.kind === "gdrive" || req.kind === "dropbox" || req.kind === "onedrive" || req.kind === "box"
              ? "Signing in… check your browser" : "Connecting…")
        Backend.connectProfile(JSON.stringify(req))
    }

    // Another program asked us to show folders or items (org.freedesktop.FileManager1).
    function showRequested(req) {
        if (pickMode) return
        for (const path of req.paths) {
            if (req.kind === "items") {
                const cut = path.lastIndexOf("/")
                newTab(cut <= 0 ? "/" : path.substring(0, cut))
                const name = path.substring(cut + 1)
                Qt.callLater(() => { if (pane) pane.pendingName = name })
            } else newTab(path)
        }
        app.show()
        app.raise()
        app.requestActivate()
    }

    function askCredentials(prompt) {
        promptBar.open({
            label: prompt.trim(),
            password: prompt.toLowerCase().indexOf("password") >= 0,
            text: "",
            accept: (t) => Backend.answerPrompt(t, false),
            cancel: () => Backend.answerPrompt("", true)
        })
    }

    function openPalette() {
        const items = commands.all().filter(c => c.palette !== false).map(c => paletteItem(c))
        pickerBox.open({
            placeholder: "type a command, or a folder name",
            items: items,
            tokenFor: engine.tokenFor,
            predict: true,
            fallback: (q) => folderItems(q),
            choose: (item, typed) => {
                refocus()
                if (typeof item.tag === "object" && item.tag.folder !== undefined) { if (pane) pane.navigate(item.tag.folder); return }
                if (Prefs.v.learnPalette) Backend.recordChoice(typed ?? "", item.tag)
                commands.run(item.tag, 1, false)
            },
            rebind: (item, seq) => bindKey(item.tag, seq),
            unbind: (item) => unbindCommand(item.tag),
            // Ctrl+A adds an alias, Ctrl+D pins the command to the context menu.
            keyHook: (e, item) => {
                if (!(e.modifiers & Qt.ControlModifier)) return false
                if (e.key === Qt.Key_A) { aliasPrompt(item.tag); return true }
                if (e.key === Qt.Key_D) { togglePin(item.tag); return true }
                return false
            }
        })
    }

    function aliasesFor(id) { return Object.keys(aliases).filter(a => aliases[a] === id) }

    readonly property var categories: ({
        cursor: "move", nav: "go", sel: "select", file: "file", view: "view", search: "search", pin: "pin",
        tab: "tab", pane: "pane", layout: "layout", devices: "devices", remote: "devices", app: "app",
        plugin: "plugin", menu: "menu", keymap: "keys", pick: "picker"
    })

    function paletteItem(c) {
        const als = aliasesFor(c.id)
        const keys = engine.keysFor(c.id).map(engine.prettySeq)
        return {
            title: c.title,
            category: categories[c.id.split(".")[0]] ?? "",
            search: c.title + " " + als.concat(c.words ?? []).join(" "),
            aliasList: als,
            keys: keys,
            bindable: true,
            hint: als.length > 0 ? "≈ " + als.join(", ") : "",
            tag: c.id
        }
    }

    function aliasPrompt(id) {
        pickerBox.close()
        promptBar.open({
            label: "Alias for “" + commands.byId(id).title + "”:",
            text: aliasesFor(id).join(" "),
            accept: (text) => {
                const a = Object.assign({}, aliases)
                for (const k of Object.keys(a)) if (a[k] === id) delete a[k]
                for (const word of text.split(/[\s,]+/)) if (word !== "") a[word] = id
                aliases = a
                Backend.writeConfig("aliases", JSON.stringify(a))
                toast(text.trim() === "" ? "Aliases cleared" : "Alias saved")
            }
        })
    }

    function togglePin(id) {
        const i = pinned.indexOf(id)
        pinned = i >= 0 ? pinned.filter(p => p !== id) : pinned.concat([id])
        markDirty()
        toast(i >= 0 ? "Unpinned" : "Pinned to the context menu")
        if (pickerBox.visible) { pickerBox.baseItems = pickerBox.baseItems.map(it => paletteItem(commands.byId(it.tag))); pickerBox.refresh() }
    }

    // Context menu: pinned actions, open-with applications, then the actions for the target.
    function contextMenu() {
        const p = pane
        if (!p) return
        const items = []
        const add = (id, extra) => {
            const c = commands.byId(id)
            if (!c) return
            const it = paletteItem(c)
            it.title = extra ?? c.title
            items.push(it)
        }
        for (const id of pinned) if (commands.byId(id)) { const c = commands.byId(id); const it = paletteItem(c); it.title = "★ " + c.title; items.push(it) }
        const hasTarget = p.currentPath !== ""
        if (hasTarget) {
            add("nav.enter", p.currentIsDir ? "Open folder" : "Open")
            let apps = []
            try { apps = JSON.parse(Backend.openWithList(p.currentPath)) } catch (e) { apps = [] }
            for (const a of apps.slice(0, 12))
                items.push({ title: "Open with " + a.name, hint: "", tag: { app: a.id, path: p.currentPath } })
            add("nav.openExternal", "Open with default application")
            add("file.copy"); add("file.cut")
        }
        add("file.paste")
        if (hasTarget) { add("file.rename"); add("file.trash"); add("file.delete") }
        if (p.dir.selectedCount > 1) add("file.batchRename")
        add("file.newFile"); add("file.newDir")
        if (hasTarget) { add("file.copyPath"); add("file.copyName") }
        add("file.terminal"); add("pin.folder")
        for (const c of commands.pluginCommands) if (c.id.indexOf("plugin.") === 0 && c.id.indexOf(".toggle") < 0) add(c.id, c.title)
        // Pinned entries appear twice otherwise.
        const seen = {}
        const unique = items.filter(it => { const k = JSON.stringify(it.tag); if (seen[k]) return false; seen[k] = true; return true })
        pickerBox.open({
            placeholder: "Actions for " + (hasTarget ? p.baseName(p.currentPath) : p.pretty(p.dir.path)) + "  (Ctrl+D pins)",
            items: unique,
            prompt: "›",
            hintRight: "context",
            choose: (item) => {
                refocus()
                if (typeof item.tag === "object") Backend.openWith(item.tag.app, item.tag.path)
                else commands.run(item.tag, 1, false)
            },
            keyHook: (e, item) => {
                if ((e.modifiers & Qt.ControlModifier) && e.key === Qt.Key_D && typeof item.tag === "string") { togglePin(item.tag); return true }
                return false
            }
        })
    }

    // Move tab `from` to position `to` (drag a tab chip, or the command).
    // Searchable list of the open tabs.
    function tabsPicker() {
        const items = tabPages.map((t, i) => ({
            title: (i + 1) + "  " + t.title,
            hint: t.activePane ? t.activePane.pretty(t.activePane.dir.path) : "",
            tag: i
        }))
        pickerBox.open({
            placeholder: "Jump to a tab",
            prompt: "⇥",
            hintRight: items.length + " tabs",
            items: items,
            choose: (item) => { refocus(); selectTab(item.tag) }
        })
    }

    function moveTab(from, to) {
        if (from === to || from < 0 || to < 0 || from >= tabsModel.count || to >= tabsModel.count) return
        tabsModel.move(from, to, 1)
        tabIndex = tabIndex === from ? to : (from < tabIndex && to >= tabIndex ? tabIndex - 1 : (from > tabIndex && to <= tabIndex ? tabIndex + 1 : tabIndex))
        Qt.callLater(() => {
            const pages = []
            for (let i = 0; i < tabsModel.count; i++) pages.push(tabRepeater.itemAt(i))
            tabPages = pages
            markDirty()
        })
    }

    // keys.json keeps overrides per keymap: { "fp": {...}, "vim": {...} }.
    function profileKeys() {
        const structured = userKeys.vim !== undefined || userKeys.fp !== undefined
        const all = structured ? Object.assign({}, userKeys) : { vim: Object.assign({}, userKeys) }
        all[keymap] = Object.assign({}, all[keymap] ?? {})
        return all
    }

    function setKeymap(name) {
        keymap = name
        engine.rebuild(userKeys)
        toast(name === "fp" ? "File Pilot keys (Ctrl shortcuts, type to find)" : "Vim keys (modal, comma leader)")
        markDirty()
    }

    function bindKey(id, seq) {
        const k = profileKeys()
        k[keymap][seq] = id
        userKeys = k
        Backend.writeConfig("keys", JSON.stringify(k))
        engine.rebuild(k)
        toast("Bound " + engine.prettySeq(seq))
        refreshPaletteHints()
    }

    function unbindCommand(id) {
        const k = profileKeys()
        for (const seq of engine.keysFor(id)) k[keymap][seq] = ""
        for (const seq in k[keymap]) if (k[keymap][seq] === id) k[keymap][seq] = ""
        userKeys = k
        Backend.writeConfig("keys", JSON.stringify(k))
        engine.rebuild(k)
        toast("Unbound keys for “" + commands.byId(id).title + "”")
        refreshPaletteHints()
    }

    function refreshPaletteHints() {
        if (!pickerBox.visible) return
        pickerBox.baseItems = pickerBox.baseItems.map(i => typeof i.tag === "string" && commands.byId(i.tag) ? paletteItem(commands.byId(i.tag)) : i)
        pickerBox.refresh()
    }

    function handleOp(json) {
        const r = JSON.parse(json)
        if (r.cancelled) toast("Cancelled" + (r.ok > 0 ? "  (" + commands.plural(r.ok, "item") + " already " + r.label.toLowerCase() + ")" : ""), "warn")
        else if (r.errors && r.errors.length > 0) toast(r.errors[0] + (r.errors.length > 1 ? "  (+" + (r.errors.length - 1) + " more)" : ""), "error")
        else if (r.ok > 0) toast(r.label + " " + commands.plural(r.ok, "item") + (r.skipped > 0 ? ", skipped " + r.skipped : "") + (r.label === "Trashed" ? "  (u to undo)" : ""))
        else if (r.skipped > 0) toast("Skipped " + commands.plural(r.skipped, "item"), "warn")
        const p = pane
        if (p && r.paths && r.paths.length > 0 && r.label !== "Trashed" && r.label !== "Deleted") {
            p.pendingPath = r.paths[r.paths.length - 1]
            if (p.dir.rowOfPath(p.pendingPath) >= 0) p.restoreCursor()
        }
        if (p && p.deep) p.dir.deepQuery(p.dir.filter)
    }

    // ---- plugins ------------------------------------------------------------------------------

    property var panelList: []            // [{key, title, qml, side}]
    property var openPanels: ({})         // key -> bool
    property var pluginCallbacks: ({})
    property int nextCallId: 1

    function loadPlugins() {
        let plugins = []
        try { plugins = JSON.parse(Backend.listPlugins()) } catch (e) { return }
        const cmds = []
        const panels = []
        pluginInfo = plugins
        for (const pl of plugins) {
            // Every plugin gets a switch command, so it can be turned back on from the palette.
            cmds.push({
                id: "plugin." + pl.id + ".switch",
                title: "Plugins: turn " + (pl.enabled === false ? "on " : "off ") + pl.name,
                words: ["enable plugin", "disable plugin", pl.id],
                keys: [],
                run: () => setPluginEnabled(pl.id, pl.enabled === false)
            })
            if (pl.enabled === false) continue
            for (const a of pl.actions) {
                cmds.push({
                    id: "plugin." + pl.id + "." + a.id,
                    title: pl.name + ": " + a.title,
                    keys: a.keys ?? [],
                    run: () => runPlugin(pl.id, a.id)
                })
            }
            for (const pn of pl.panels) {
                const key = pl.id + "." + pn.id
                panels.push({ key: key, title: pn.title, qml: pn.qml, side: pn.side })
                cmds.push({
                    id: "plugin." + key + ".toggle",
                    title: pl.name + ": toggle " + pn.title,
                    keys: [],
                    run: () => togglePanel(key)
                })
            }
        }
        commands.pluginCommands = cmds
        panelList = panels
        engine.rebuild(userKeys)
    }

    property var pluginInfo: []

    // Switch plugins on or off (Settings > Plugins, the palette, `captainfm --plugin`). Previews follow at once;
    // actions and panels are reloaded here.
    function setPluginsEnabled(ids, on) {
        let off = Prefs.v.disabledPlugins.filter(id => ids.indexOf(id) < 0)
        if (!on) off = off.concat(ids)
        Prefs.set("disabledPlugins", off)
        loadPlugins()
    }
    function setPluginEnabled(id, on) {
        setPluginsEnabled([id], on)
        const pl = pluginInfo.find(p => p.id === id)
        toast((pl ? pl.name : id) + (on ? " on" : " off"))
    }
    function setAllPlugins(on) {
        setPluginsEnabled(pluginInfo.map(p => p.id), on)
        toast("All plugins " + (on ? "on" : "off"))
    }

    // `captainfm --plugin` changes settings.json from outside: pick that up when the window is used again.
    Connections {
        target: Qt.application
        function onStateChanged() {
            if (Qt.application.state !== Qt.ApplicationActive) return
            const before = JSON.stringify(Prefs.v.disabledPlugins)
            Prefs.load()
            if (JSON.stringify(Prefs.v.disabledPlugins) !== before) app.loadPlugins()
        }
    }

    function togglePanel(key) {
        const o = Object.assign({}, openPanels)
        o[key] = !o[key]
        openPanels = o
    }

    function pluginContext() {
        const p = pane
        return {
            path: p ? p.dir.path : "",
            cursor: p ? p.currentPath : "",
            selection: p ? JSON.parse(p.dir.selectedPaths()) : [],
            home: Backend.home()
        }
    }

    function runPlugin(plugin, action, done) {
        const id = nextCallId++
        if (done) pluginCallbacks[id] = done
        Backend.runPlugin(plugin, action, JSON.stringify(pluginContext()), id)
    }

    function applyEffects(callId, json) {
        const effects = JSON.parse(json)
        const cb = pluginCallbacks[callId]
        delete pluginCallbacks[callId]
        for (const e of effects) {
            switch (e.type) {
            case "toast": toast(e.message, e.kind); break
            case "navigate": if (pane) pane.navigate(e.path); break
            case "select":
                if (pane) { pane.pendingPath = e.path; if (pane.dir.rowOfPath(e.path) >= 0) pane.restoreCursor() }
                break
            case "open": Backend.openPath(e.path); break
            case "copy": Backend.copyText(e.text); break
            case "command": commands.run(e.id, 1, false); break
            case "data":
                if (cb) cb(e.value)
                else if (typeof e.value === "string") toast(e.value)
                break
            }
        }
    }

    // ---- wiring -------------------------------------------------------------------------

    Commands { id: commandsRef; app: app }
    PluginApi { id: pluginApi; app: app }
    KeyEngine { id: engine; commands: app.commands; profile: app.keymap }

    Connections {
        target: Backend
        function onOpDone(result) { app.handleOp(result) }
        function onTransferStarted() { transferBox.refresh() }
        function onPasteConflicts(json) { clashBox.ask(JSON.parse(json)) }
        function onDevicesListed(json) { app.showDevices(json) }
        function onMountDone(json) { app.handleMount(json) }
        function onCredentialsNeeded(prompt) { app.askCredentials(prompt) }
        function onFileManagerRequest(json) { app.showRequested(JSON.parse(json)) }
        function onPluginResult(callId, json) { app.applyEffects(callId, json) }
    }

    onShowHiddenChanged: markDirty()
    onSortKeyChanged: markDirty()
    onSortDescChanged: markDirty()
    onInspectorOnChanged: markDirty()
    onClosing: saveSession()

    // The index only changes while a deep search is open, so count it only then (and once more when it closes).
    function updateIndexText() {
        const n = Backend.indexPaths()
        indexText = n <= 0 ? "" : (n >= 1000000 ? (n / 1000000).toFixed(2) + "M" : (n >= 1000 ? (n / 1000).toFixed(1) + "k" : String(n)))
    }
    Timer {
        interval: 1000; running: app.pane !== null && app.pane.deep; repeat: true; triggeredOnStart: true
        onTriggered: app.updateIndexText()
        onRunningChanged: if (!running) app.updateIndexText()
    }
    onSidebarModeChanged: markDirty()
    onKeymapChanged: engine.rebuild(userKeys)
    onGroupByChanged: markDirty()

    Timer { id: saveTimer; interval: 1500; onTriggered: app.saveSession() }

    // CFM_DEBUG=1 prints how long startup took: until the interface was built, and until the first frame.
    readonly property bool debugStartup: Backend.env("CFM_DEBUG") !== ""
    property bool firstFrameSeen: false
    onFrameSwapped: {
        if (firstFrameSeen) return
        firstFrameSeen = true
        if (debugStartup) console.log("[timing] first frame " + Backend.sinceStartMs() + " ms after start")
    }

    Component.onCompleted: {
        Backend.markFeature("app.opened")
        if (debugStartup) console.log("[timing] interface built " + Backend.sinceStartMs() + " ms after start")
        Prefs.load()
        Theme.start()
        try { userKeys = JSON.parse(Backend.readConfig("keys")) } catch (e) { userKeys = ({}) }
        engine.rebuild(userKeys)
        loadPlugins()

        // Picker mode: `--pick <json>` (used by the file chooser portal).
        const argvAll = Qt.application.arguments
        const pickAt = argvAll.indexOf("--pick")
        if (pickAt >= 0 && pickAt + 1 < argvAll.length) {
            try { pickReq = JSON.parse(argvAll[pickAt + 1]) } catch (e) { pickReq = ({}) }
            pickMode = true
            app.title = pickReq.title ?? "Choose"
            app.width = 960
            app.height = 640
            inspectorOn = true
            pickBar.req = pickReq
            if (pickReq.save) pickBar.setName(pickReq.name ?? "")
            newTab(pickReq.folder && Backend.isDir(pickReq.folder) ? pickReq.folder : Backend.home())
            ready = true
            keyRoot.forceActiveFocus()
            Backend.startServices()
            Qt.callLater(applyPickFilter)
            automation.start()
            return
        }

        // Positional paths open as tabs; `--select <path>` opens the parent with the item highlighted.
        const argv = Qt.application.arguments.slice(1)
        const args = []
        let selectPath = ""
        for (let i = 0; i < argv.length; i++) {
            if (argv[i] === "--select" && i + 1 < argv.length) selectPath = argv[++i]
            else if (argv[i].length > 0 && argv[i][0] !== "-") args.push(argv[i])
        }
        let session = {}
        try { session = JSON.parse(Backend.readConfig("session")) } catch (e) { session = ({}) }
        if (session.inspector !== undefined) inspectorOn = session.inspector
        if (session.showHidden !== undefined) showHidden = session.showHidden
        if (session.sortKey) sortKey = session.sortKey
        if (session.sortDesc !== undefined) sortDesc = session.sortDesc
        if (session.zoom) Style.zoomTarget = session.zoom
        if (session.animations !== undefined) Style.animations = session.animations
        if (session.density) Style.density = session.density
        if (session.pinned) pinned = session.pinned
        if (session.keymap) keymap = session.keymap
        if (session.groupBy) groupBy = session.groupBy
        if (session.sidebarMode) sidebarMode = session.sidebarMode
        else if (session.sidebar === false) sidebarMode = "hidden"
        if (session.folds) sidebar.folded = session.folds
        if (Backend.env("CFM_KEYMAP") !== "") keymap = Backend.env("CFM_KEYMAP")
        try { aliases = JSON.parse(Backend.readConfig("aliases")) } catch (e) { aliases = ({}) }

        if (selectPath !== "") {
            const full = Backend.expandPath(selectPath).replace(/^file:\/\//, "")
            const cut = full.lastIndexOf("/")
            newTab(cut <= 0 ? "/" : full.substring(0, cut))
            Qt.callLater(() => { if (pane) pane.pendingName = full.substring(cut + 1) })
        } else if (args.length > 0) {
            for (const a of args) newTab(Backend.expandPath(a).replace(/^file:\/\//, ""))
            tabIndex = 0
        } else if (Prefs.v.startup !== "restore") {
            const folder = Prefs.v.startup === "folder" ? Backend.expandPath(Prefs.v.startFolder) : ""
            newTab(folder !== "" && Backend.isDir(folder) ? folder : Backend.home())
        } else if (session.tabs && session.tabs.length > 0) {
            applySnapshot(session)
        } else {
            newTab(Backend.home())
        }
        ready = true
        keyRoot.forceActiveFocus()
        Backend.startServices()
        automation.start()
    }

    // ---- layout -------------------------------------------------------------------------

    Item {
        id: keyRoot
        anchors.fill: parent
        focus: true

        Keys.onPressed: (e) => {
            // The Enter that confirmed a palette/prompt choice must not also open the item underneath.
            if ((e.key === Qt.Key_Return || e.key === Qt.Key_Enter) && Date.now() - app.overlayClosedAt < 250) {
                e.accepted = true
                return
            }
            if (engine.handle(e)) e.accepted = true
        }

        ColumnLayout {
            anchors.fill: parent
            spacing: 0

            // Header: mode badge, breadcrumbs (or the search query) and counts.
            Rectangle {
                id: topBar
                Layout.fillWidth: true
                Layout.preferredHeight: Math.round(38 * Style.zoom)
                color: Style.bg
                Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: Style.surface }

                readonly property bool deep: app.pane && app.pane.deep
                readonly property bool visual: app.pane && app.pane.visualMode

                // Sidebar toggle: open -> icons only -> hidden.
                Rectangle {
                    id: sidebarButton
                    anchors { left: parent.left; leftMargin: Style.pad; verticalCenter: parent.verticalCenter }
                    width: Math.round(28 * Style.zoom); height: width; radius: 5
                    color: sbArea.pressed ? Style.selection : (sbArea.containsMouse ? Style.alpha(Style.selection, 0.6) : "transparent")
                    Text {
                        anchors.centerIn: parent
                        // Three states drawn as a miniature sidebar: wide, narrow, none.
                        text: app.sidebarMode === "open" ? Style.pick("\uf0c9", "☰")
                              : (app.sidebarMode === "icons" ? Style.pick("\uf0ca", "☷") : Style.pick("\uf105", "›"))
                        color: sbArea.containsMouse ? Style.fg : Style.fgLight
                        font { family: Style.font; pixelSize: Style.fontSize + 2 }
                    }
                    MouseArea {
                        id: sbArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: app.cycleSidebar()
                    }
                }

                Rectangle {
                    id: badge
                    anchors { left: sidebarButton.right; leftMargin: Style.pad / 2; verticalCenter: parent.verticalCenter }
                    height: Math.round(22 * Style.zoom)
                    width: badgeText.implicitWidth + Style.pad * 2
                    radius: 4
                    color: topBar.deep ? Style.accent : (topBar.visual ? Style.magenta : Style.alpha(Style.accent, 0.3))
                    Behavior on color { ColorAnimation { duration: Style.anim(120) } }
                    Text {
                        id: badgeText
                        anchors.centerIn: parent
                        text: topBar.deep ? "SEARCH" : (topBar.visual ? (app.keymap === "fp" ? "SELECT" : "VISUAL")
                                                                          : (app.keymap === "fp" ? "FILES" : "NORMAL"))
                        color: topBar.deep || topBar.visual ? Style.bgDarker : Style.fg
                        font { family: Style.font; pixelSize: Style.smallFont; bold: true }
                    }
                }

                Breadcrumbs {
                    anchors { left: badge.right; leftMargin: Style.pad * 1.2; right: topInfo.left; rightMargin: Style.pad
                              verticalCenter: parent.verticalCenter }
                    height: parent.height
                    path: app.pane ? app.pane.dir.path : ""
                    trailing: topBar.deep && app.pane ? (app.pane.searchText + "\u2588") : ""
                    onNavigate: (target) => { if (app.pane) app.pane.navigate(target) }
                }

                Text {
                    id: topInfo
                    anchors { right: parent.right; rightMargin: Style.pad * 1.5; verticalCenter: parent.verticalCenter }
                    color: Style.fgDim
                    font { family: Style.font; pixelSize: Style.fontSize - 1 }
                    text: topBar.deep ? "enter open · ctrl+enter new tab · alt+enter reveal"
                          : (app.pane ? (app.pane.dir.selectedCount > 0 ? app.pane.dir.selectedCount + " selected · " : "")
                                        + app.pane.dir.entryCount + (app.pane.dir.entryCount === 1 ? " item" : " items") : "")
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: 0

                Sidebar {
                    id: sidebar
                    Layout.fillHeight: true
                    Layout.preferredWidth: app.sidebarMode === "hidden" ? 0
                                           : (compact ? Math.round(46 * Style.zoom) : Math.round(196 * Style.zoom))
                    visible: Layout.preferredWidth > 1
                    compact: app.sidebarMode === "icons" || app.width < 640
                    currentPath: app.pane ? app.pane.dir.path : ""
                    onNavigate: (path) => {
                        if (!app.pane) return
                        if (Backend.isDir(path)) app.pane.navigate(path); else if (!app.pickMode) Backend.openPath(path)
                    }
                    onConnectRequested: app.connectPrompt()
                    armedKey: app.armedKey
                    onUnpinRequested: (path, name) => app.togglePinned(path, name)
                    onDeviceRightClicked: (item) => app.deviceRightClicked(item)
                    onFoldsChanged: app.markDirty()
                    searchValue: app.pane && app.pane.deep ? app.pane.searchText : ""
                    onSearchChanged: (text) => app.sidebarSearch(text)
                    onSearchAccepted: app.refocus()
                    onSearchCancelled: { if (app.pane && app.pane.deep) app.pane.exitDeep(); app.refocus() }
                    onSearchRequested: commands.run("search.drive", 1, false)
                    Behavior on Layout.preferredWidth { NumberAnimation { duration: Style.anim(160); easing.type: Easing.OutCubic } }
                }

                Item {
                    id: tabsHost
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    // Numbered tabs inside the content column.
                    Rectangle {
                        id: tabStrip
                        z: 2
                        anchors { left: parent.left; right: parent.right; top: parent.top }
                        height: Math.round(32 * Style.zoom)
                        color: Style.bg
                        Row {
                            id: tabRow
                            anchors { fill: parent; leftMargin: Style.pad }
                            spacing: 2
                            Repeater {
                                model: tabsModel.count
                                delegate: Rectangle {
                                    id: chip
                                    required property int index
                                    readonly property bool current: index === app.tabIndex
                                    readonly property bool armed: app.armedKey === "tab:" + index
                                    readonly property Item page: app.tabPages.length > index ? app.tabPages[index] : null
                                    anchors.verticalCenter: parent.verticalCenter
                                    height: parent.height - 8
                                    width: Math.min(Math.round(220 * Style.zoom), label.implicitWidth + Style.pad * 2.4)
                                    radius: 4
                                    color: current ? Style.surface : "transparent"
                                    Text {
                                        id: label
                                        anchors.centerIn: parent
                                        textFormat: Text.RichText
                                        text: chip.armed ? "<font color=\"" + Style.red + "\">close tab " + (chip.index + 1) + "?</font>"
                                              : "<font color=\"" + (chip.current ? Style.accent : Style.fgDim) + "\">" + (chip.index + 1) + "</font> "
                                                + (chip.page ? chip.page.title.replace(/&/g, "&amp;").replace(/</g, "&lt;") : "")
                                        color: chip.current ? Style.fg : Style.fgDim
                                        elide: Text.ElideRight
                                        width: Math.min(implicitWidth, chip.width - Style.pad)
                                        font { family: Style.font; pixelSize: Style.fontSize }
                                    }
                                    MouseArea {
                                        anchors.fill: parent
                                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                                        onClicked: (m) => {
                                            if (m.button === Qt.RightButton) app.tabRightClicked(chip.index, chip.page ? chip.page.title : String(chip.index + 1))
                                            else app.selectTab(chip.index)
                                        }
                                    }
                                    // Drag a tab sideways to reorder; it drops where its centre lands.
                                    DragHandler {
                                        yAxis.enabled: false
                                        cursorShape: Qt.ClosedHandCursor
                                        onActiveChanged: {
                                            if (active) return
                                            const centre = chip.x + chip.width / 2
                                            let target = 0
                                            const siblings = chip.parent.children
                                            for (let i = 0; i < siblings.length; i++) {
                                                const c = siblings[i]
                                                if (c !== chip && c.hasOwnProperty("index") && c.x + c.width / 2 < centre) target++
                                            }
                                            chip.parent.forceLayout()
                                            app.moveTab(chip.index, target)
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Repeater {
                        id: tabRepeater
                        model: tabsModel
                        delegate: TabPage {
                            required property int index
                            required property string paths
                            required property string views
                            required property string layout
                            anchors { fill: parent; topMargin: tabStrip.height }
                            visible: index === app.tabIndex
                            current: visible
                            startPaths: JSON.parse(paths)
                            startViews: JSON.parse(views)
                            layoutMode: layout
                            optHidden: app.showHidden
                            optGroupBy: app.groupBy
                            optSortKey: app.sortKey
                            optSortDesc: app.sortDesc
                            onZoomRequested: (s) => commands.zoom(0.1 * s)
                            onContextRequested: app.rowRightClicked()
                            onSortRequested: (key) => commands.setSort(key)
                            onScopeToggled: commands.search(!app.pane.dir.path || app.pane.dir.path !== Backend.mountRoot(app.pane.dir.path))
                            onPickRequested: app.pickEnterFile()
                        }
                        onItemAdded: (i, item) => {
                            const a = app.tabPages.slice()
                            a.splice(i, 0, item)
                            app.tabPages = a
                        }
                        onItemRemoved: (i, item) => {
                            app.tabPages = app.tabPages.filter(t => t !== item)
                        }
                    }
                }

                Repeater {
                    model: app.panelList.filter(p => p.side === "right")
                    delegate: Item {
                        id: sidePanel
                        required property var modelData
                        readonly property bool open: app.openPanels[modelData.key] === true
                        Layout.fillHeight: true
                        Layout.preferredWidth: open ? Math.round(320 * Style.zoom) : 0
                        visible: open
                        clip: true
                        Rectangle { anchors.fill: parent; color: Style.bgDark }
                        Rectangle { width: 1; height: parent.height; color: Style.surface }
                        Loader {
                            anchors.fill: parent
                            active: sidePanel.open
                            source: sidePanel.modelData.qml
                            onLoaded: if ("api" in item) item.api = pluginApi
                            onStatusChanged: if (status === Loader.Error) app.toast("Plugin panel failed to load: " + sidePanel.modelData.title, "error")
                        }
                    }
                }

                Inspector {
                    id: inspectorPane
                    hotkeyNote: app.mediaHotkeys()
                    onActionRequested: (id) => commands.run(id, 1, false)
                    Layout.fillHeight: true
                    Layout.preferredWidth: app.inspectorOn && app.width > 760 ? Math.min(Math.round(420 * Style.zoom), app.width * 0.34) : 0
                    visible: Layout.preferredWidth > 1
                    clip: true
                    path: visible && app.pane ? app.pane.currentPath : ""
                    Behavior on Layout.preferredWidth { NumberAnimation { duration: Style.anim(160); easing.type: Easing.OutCubic } }
                }
            }

            Repeater {
                model: app.panelList.filter(p => p.side === "bottom")
                delegate: Item {
                    id: bottomPanel
                    required property var modelData
                    readonly property bool open: app.openPanels[modelData.key] === true
                    Layout.fillWidth: true
                    Layout.preferredHeight: open ? Math.round(180 * Style.zoom) : 0
                    visible: open
                    clip: true
                    Rectangle { anchors.fill: parent; color: Style.bgDark }
                    Rectangle { width: parent.width; height: 1; color: Style.surface }
                    Loader {
                        anchors.fill: parent
                        active: bottomPanel.open
                        source: bottomPanel.modelData.qml
                        onLoaded: if ("api" in item) item.api = pluginApi
                        onStatusChanged: if (status === Loader.Error) app.toast("Plugin panel failed to load: " + bottomPanel.modelData.title, "error")
                    }
                }
            }

            PickBar {
                id: pickBar
                Layout.fillWidth: true
                visible: app.pickMode
                Layout.preferredHeight: visible ? height : 0
                onAccepted: app.pickAccept()
                onCancelled: Backend.pickCancel()
                onFilterCycled: app.pickCycleFilter()
            }

            PromptBar {
                id: promptBar
                Layout.fillWidth: true
                onClosed: app.refocus()
            }

            // Status line: key hints on the left, index state and zoom on the right.
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: Math.round(28 * Style.zoom)
                color: Style.bg
                Rectangle { anchors { left: parent.left; right: parent.right; top: parent.top } height: 1; color: Style.surface }

                readonly property bool deep: app.pane && app.pane.deep
                readonly property var hints: app.keymap === "fp"
                    ? (deep ? [["↑↓", "move"], ["enter", "open"], ["esc", "back"]]
                            : [["↑↓", "move"], ["→", "open"], ["←", "up"], ["ctrl+l", "go to"], ["ctrl+f", "search"], ["ctrl+p", "actions"]])
                    : (deep ? [["j/k", "move"], ["enter", "open"], ["esc", "back"]]
                            : [["j/k", "move"], ["l", "open"], ["h", "up"], [",", "leader"], ["/", "filter"]])

                Row {
                    id: leftStatus
                    anchors { left: parent.left; leftMargin: Style.pad * 1.5; verticalCenter: parent.verticalCenter }
                    spacing: Style.pad * 1.4
                    Text {
                        visible: engine.pending !== ""
                        text: engine.prettySeq(engine.pending)
                        color: Style.yellow
                        font { family: Style.font; pixelSize: Style.fontSize; bold: true }
                    }
                    Repeater {
                        model: parent.parent.hints
                        delegate: Text {
                            required property int index
                            required property var modelData
                            textFormat: Text.RichText
                            text: "<font color=\"" + (index === 0 ? Style.accent : Style.fgLight) + "\"><b>" + modelData[0] + "</b></font>"
                                  + "<font color=\"" + Style.fgDim + "\"> " + modelData[1] + "</font>"
                            font { family: Style.font; pixelSize: Style.smallFont + 1 }
                        }
                    }
                }

                Row {
                    id: rightStatus
                    anchors { right: parent.right; rightMargin: Style.pad * 1.5; verticalCenter: parent.verticalCenter }
                    spacing: Style.pad * 1.5
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        color: Style.fgDim
                        font { family: Style.font; pixelSize: Style.smallFont + 1 }
                        text: app.indexText !== "" ? "index live · " + app.indexText + " paths" : "index live · inotify"
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        color: Style.fgDim
                        font { family: Style.font; pixelSize: Style.smallFont }
                        text: (app.showHidden ? "hidden  " : "") + app.sortKey + (app.sortDesc ? " ↓" : " ↑")
                    }
                    // Zoom slider
                    Item {
                        id: zoomSlider
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.round(80 * Style.zoom)
                        height: Math.round(18 * Style.zoom)
                        readonly property real minZoom: 0.7
                        readonly property real maxZoom: 2.2
                        Rectangle {
                            anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter }
                            height: 3; radius: 2; color: Style.surface
                            Rectangle {
                                width: handle.x + handle.width / 2; height: parent.height; radius: 2; color: Style.accent
                            }
                        }
                        Rectangle {
                            id: handle
                            width: Math.round(10 * Style.zoom); height: width; radius: width / 2
                            anchors.verticalCenter: parent.verticalCenter
                            color: Style.fg
                            x: (Style.zoomTarget - zoomSlider.minZoom) / (zoomSlider.maxZoom - zoomSlider.minZoom)
                               * (zoomSlider.width - width)
                            Behavior on x { NumberAnimation { duration: Style.anim(140); easing.type: Easing.OutCubic } }
                        }
                        MouseArea {
                            anchors.fill: parent
                            function setFrom(mx) {
                                const f = Math.max(0, Math.min(1, (mx - handle.width / 2) / (zoomSlider.width - handle.width)))
                                Style.zoomTarget = Math.round((zoomSlider.minZoom + f * (zoomSlider.maxZoom - zoomSlider.minZoom)) * 20) / 20
                            }
                            onPressed: (m) => setFrom(m.x)
                            onPositionChanged: (m) => { if (pressed) setFrom(m.x) }
                            onDoubleClicked: Style.zoomTarget = 1.0
                        }
                    }
                }
            }
        }

        // Overlays
        LeaderHints {
            id: hints
            visible: engine.hintVisible
            items: engine.hintItems
            prefix: engine.prettySeq(engine.hintPrefix)
            anchors { right: parent.right; rightMargin: Style.pad * 2; bottom: parent.bottom; bottomMargin: Math.round(40 * Style.zoom) }
        }

        JumpBar {
            id: jumpBar
            onChosen: (path) => { if (app.pane) app.pane.navigate(path); app.refocus() }
            onClosed: app.refocus()
        }

        Picker {
            id: pickerBox
            onClosed: app.refocus()
            // The second right-click of a file or folder delete can land on the menu itself.
            onRightClicked: if (app.armedKey.indexOf("item:") === 0) { close(); app.runArmed() }
        }

        Loader {
            id: batchLoader
            anchors.fill: parent
            active: false
            // A Loader resizes what it loads, so the dialog sits in a full-size holder and keeps its own size.
            sourceComponent: Item {
                property alias dlg: dialog
                property var owner: app
                BatchRename { id: dialog; app: owner; onClosed: app.refocus() }
            }
        }

        Loader {
            id: settingsLoader
            anchors.fill: parent
            active: false
            // A Loader resizes what it loads, so the dialog sits in a full-size holder and keeps its own size.
            sourceComponent: Item {
                property alias dlg: dialog
                property var owner: app
                Settings { id: dialog; app: owner; onClosed: app.refocus() }
            }
        }

        Loader {
            id: connectLoader
            anchors.fill: parent
            active: false
            // A Loader resizes what it loads, so the dialog sits in a full-size holder and keeps its own size.
            sourceComponent: Item {
                property alias dlg: dialog
                property var owner: app
                ConnectDialog { id: dialog; app: owner; onClosed: app.refocus() }
            }
        }

        ConflictDialog {
            id: clashBox
            onClosed: app.refocus()
        }

        TransferPanel {
            id: transferBox
            app: app
            anchors { left: parent.left; bottom: parent.bottom; leftMargin: Style.pad * 2; bottomMargin: Math.round(48 * Style.zoom) }
        }

        Toast {
            id: toastBox
            anchors { right: parent.right; bottom: parent.bottom; rightMargin: Style.pad * 2; bottomMargin: Math.round(48 * Style.zoom) }
        }
    }

    // ---- test automation ----------------------------------------------------------------
    // CFM_KEYS="j j @nav.jump" feeds keys/commands; CFM_SHOT=file.png saves a screenshot and quits.

    QtObject {
        id: automation
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
            if (debugStartup && t !== "<F9>") console.log("[keys] " + Backend.sinceStartMs() + " " + t)
            if (t[0] === "@") commands.run(t.substring(1), 1, false)
            else if (t[0] === "=") {
                const f = app.activeFocusItem
                if (f && f.text !== undefined) f.text = f.text + t.substring(1).replace(/_/g, " ")
            } else if (t === "!accept") {
                const f = app.activeFocusItem
                if (f && f.accepted) f.accepted()
            } else if (t.indexOf("!bind:") === 0) {
                const [cid, seq] = t.substring(6).split("=")
                bindKey(cid, seq.replace(/_/g, " "))
            } else if (t.indexOf("!mode:") === 0) lazyBatch().setMode(t.substring(6))
            else if (t.indexOf("!fold:") === 0) sidebar.toggleFold(t.substring(6))
            else if (t.indexOf("!clash:") === 0) clashBox.press(t.substring(7))
            else if (t.indexOf("!set:") === 0) lazySettings().press(t.substring(5))
            else if (t.indexOf("!conn:") === 0) lazyConnect().press(t.substring(6))
            else if (t.indexOf("!rclick:") === 0) {
                // Test automation for the double right-click: !rclick:row, !rclick:tab:N, !rclick:pin:/path, !rclick:fake-remote, !rclick:root
                const w = t.substring(8)
                if (w === "row") rowRightClicked()
                else if (w.indexOf("tab:") === 0) tabRightClicked(parseInt(w.substring(4)), "tab " + w.substring(4))
                else if (w.indexOf("pin:") === 0) togglePinned(w.substring(4), "pin")
                else if (w.indexOf("side:") === 0) {
                    // a real sidebar drive or remote whose name contains the text
                    const found = sidebar.drives.concat(sidebar.remotes).find(x => x.label.indexOf(w.substring(5)) >= 0 || (x.path || "").indexOf(w.substring(5)) >= 0)
                    if (found) deviceRightClicked(found); else toast("No sidebar entry “" + w.substring(5) + "”", "error")
                }
                else if (w === "root") deviceRightClicked({ label: "root", path: "/", dev: { kind: "disk" }, verb: "unmount" })
                else deviceRightClicked({ label: w, path: "/fake/" + w, dev: { kind: "smb", path: "/fake/" + w }, verb: "disconnect" })
            }
            else if (t.indexOf("!space:") === 0) {
                // Test automation for the remote capacity lookup (a hover cannot be sent): !space:<part of the name>
                const found = sidebar.remotes.find(x => x.label.indexOf(t.substring(7)) >= 0 || (x.path || "").indexOf(t.substring(7)) >= 0)
                if (found) sidebar.askSpace(found); else toast("No remote “" + t.substring(7) + "”", "error")
            }
            else if (t === "!play") inspectorPane.togglePlayback()
            else if (t === "!clear") { const f = app.activeFocusItem; if (f && f.text !== undefined) f.text = "" }
            else if (t === "!next") lazyBatch().nextField(false)
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
}
