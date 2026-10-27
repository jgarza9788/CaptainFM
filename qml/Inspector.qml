import QtQuick
import CaptainFM
import QtQuick.Layouts
import QtMultimedia

// Side preview of whatever is under the cursor.
Rectangle {
    id: insp
    color: Style.bgDark

    property string path: ""
    property var info: null
    property string requested: ""

    // Which hotkeys to advertise under the player (filled in by the main window).
    property string hotkeyNote: ""
    signal actionRequested(string commandId)

    readonly property bool hasMedia: info !== null && (info.kind === "video" || info.kind === "audio")
    property bool muted: false
    property bool looping: false

    // Playback controls, shared by the on-screen buttons and the hotkey commands.
    function togglePlayback() { if (!hasMedia) return; if (player.playbackState === MediaPlayer.PlayingState) player.pause(); else player.play() }
    function play() { if (hasMedia && player.playbackState !== MediaPlayer.PlayingState) player.play() }
    function seekBy(ms) {
        if (!hasMedia || player.duration <= 0) return
        media.scrubbed = true
        player.position = Math.max(0, Math.min(player.duration, player.position + ms))
    }
    function toggleMute() { muted = !muted }
    function toggleLoop() { looping = !looping }
    // The player is made the first time something is played or previewed, not at start-up: creating it loads
    // the FFmpeg backend, about 90 ms of the interface's start (QML Profiler, todo 17.6). Kept once made.
    property bool mediaUsed: false
    onHasMediaChanged: if (hasMedia) mediaUsed = true
    readonly property var player: playerLoader.item
    readonly property var mediaPlayer: player

    onPathChanged: {
        if (path === "") { info = null; return }
        debounce.restart()
    }

    Timer {
        id: debounce
        interval: 60
        onTriggered: { insp.requested = insp.path; Backend.requestPreview(insp.path) }
    }

    Connections {
        target: Backend
        function onPreviewReady(previewPath, json) {
            if (previewPath === insp.path) insp.info = JSON.parse(json)
        }
    }

    Rectangle { x: 0; y: 0; width: 1; height: parent.height; color: Style.surface }

    // ---- header -------------------------------------------------------------------------

    Item {
        id: head
        anchors { left: parent.left; right: parent.right; top: parent.top }
        height: Math.round(56 * Style.zoom)
        visible: insp.info !== null

        Text {
            id: kindIcon
            x: Style.pad * 1.5
            anchors { top: parent.top; topMargin: Style.pad }
            width: Style.fontSize * 1.8
            horizontalAlignment: Text.AlignHCenter
            text: insp.info ? Style.glyph(insp.info.name, insp.info.kind === "dir", insp.info.isLink) : ""
            color: insp.info ? Style.glyphColor(insp.info.name, insp.info.kind === "dir") : Style.fgDim
            font { family: Style.font; pixelSize: Style.fontSize + 3 }
        }
        Text {
            id: nameText
            anchors { left: kindIcon.right; leftMargin: Style.pad / 2; right: actions.left; rightMargin: Style.pad
                      top: parent.top; topMargin: Style.pad }
            text: insp.info ? Style.oneLine(insp.info.name) : ""
            textFormat: Text.PlainText
            elide: Text.ElideMiddle
            color: Style.fg
            font { family: Style.font; pixelSize: Style.fontSize + 1; bold: true }
        }
        Text {
            anchors { left: nameText.left; right: parent.right; rightMargin: Style.pad
                      top: nameText.bottom; topMargin: 2 }
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: Style.fgDim
            font { family: Style.font; pixelSize: Style.smallFont }
            text: {
                const i = insp.info
                if (!i) return ""
                const parts = []
                if (i.size >= 0) parts.push(Style.humanSize(i.size))
                if (i.kind === "dir") parts.push(i.total + (i.total === 1 ? " item" : " items"))
                if (i.kind === "text") parts.push(i.lines + " lines" + (i.truncated ? "+" : ""))
                if (i.kind === "image" && i.info && i.info.length > 0) parts.push(i.info[0])
                if (i.note) parts.push(i.note)
                parts.push(i.perms + (i.isLink ? " → " + i.linkTarget : ""))
                return parts.join(" · ")
            }
        }

        // Quick actions for the previewed item.
        Row {
            id: actions
            anchors { right: parent.right; rightMargin: Style.pad; top: parent.top; topMargin: Style.pad - 2 }
            spacing: 2
            IconButton { glyph: Style.pick("\uf08e", "↗"); tip: "open"; onClicked: insp.actionRequested("nav.openExternal") }
            IconButton { glyph: Style.pick("\uf0c5", "⧉"); tip: "copy path"; onClicked: insp.actionRequested("file.copyPath") }
            IconButton { glyph: Style.pick("\uf005", "★"); tip: "pin"; onClicked: insp.actionRequested("pin.item") }
        }
    }

    // Small icon button used in the header and the player.
    component IconButton: Rectangle {
        id: btn
        property string glyph: ""
        property string tip: ""
        property bool active: false
        property real size: Math.round(26 * Style.zoom)
        signal clicked()
        width: size; height: size; radius: 5
        color: area.pressed ? Style.selection : (area.containsMouse ? Style.alpha(Style.selection, 0.6) : "transparent")
        Text {
            anchors.centerIn: parent
            text: btn.glyph
            color: btn.active ? Style.accent : (area.containsMouse ? Style.fg : Style.fgLight)
            font { family: Style.font; pixelSize: Style.fontSize }
        }
        MouseArea {
            id: area
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: btn.clicked()
        }
    }

    // ---- body ---------------------------------------------------------------------------

    Item {
        id: body
        anchors { left: parent.left; right: parent.right; top: head.bottom; bottom: parent.bottom }
        anchors.margins: Style.pad
        clip: true

        // Text, highlighted.
        Flickable {
            anchors.fill: parent
            visible: insp.info && insp.info.kind === "text"
            contentWidth: codeText.implicitWidth
            contentHeight: codeText.implicitHeight
            boundsBehavior: Flickable.StopAtBounds
            Text {
                id: codeText
                textFormat: Text.RichText
                text: insp.info && insp.info.kind === "text" ? insp.info.html : ""
                color: Style.fg
                font { family: Style.font; pixelSize: Style.smallFont }
            }
        }

        Image {
            anchors.fill: parent
            visible: insp.info && insp.info.kind === "image"
            source: insp.info && insp.info.kind === "image" ? insp.info.url : ""
            fillMode: Image.PreserveAspectFit
            asynchronous: true
            autoTransform: true
            sourceSize.width: 900
            sourceSize.height: 900
            horizontalAlignment: Image.AlignHCenter
            verticalAlignment: Image.AlignTop
        }

        ListView {
            id: entryList
            anchors.fill: parent
            visible: insp.info && (insp.info.kind === "dir" || insp.info.kind === "archive")
            model: visible ? insp.info.entries : []
            boundsBehavior: Flickable.StopAtBounds
            delegate: Text {
                required property var modelData
                readonly property bool isObj: typeof modelData === "object"
                readonly property string label: isObj ? modelData.name : modelData
                readonly property bool isDir: isObj ? modelData.isDir : label.endsWith("/")
                width: ListView.view.width
                height: Math.round(22 * Style.zoom)
                elide: Text.ElideMiddle
                text: Style.glyph(label, isDir, false) + "  " + label
                textFormat: Text.PlainText
                color: isDir ? Style.accent : Style.fgLight
                font { family: Style.font; pixelSize: Style.smallFont }
            }
        }

        // Video and audio: poster, transport controls, seek bar and a hotkey note.
        Item {
            id: media
            anchors.fill: parent
            visible: insp.hasMedia
            property bool scrubbed: false          // the video frame (not the poster) is on screen
            readonly property bool playing: insp.player !== null && insp.player.playbackState === MediaPlayer.PlayingState
            readonly property real duration: insp.player ? insp.player.duration : 0
            readonly property real position: insp.player ? insp.player.position : 0
            readonly property bool isVideo: insp.info !== null && insp.info.kind === "video"
            readonly property bool showVideo: isVideo && (playing || scrubbed)
            Connections { target: insp; function onInfoChanged() { media.scrubbed = false } }

            function fmt(ms) {
                const s = Math.max(0, Math.floor(ms / 1000))
                const pad = n => (n < 10 ? "0" : "") + n
                return Math.floor(s / 60) + ":" + pad(s % 60)
            }

            Loader {
                id: playerLoader
                active: insp.mediaUsed
                sourceComponent: MediaPlayer {
                    id: mp
                    source: media.visible ? insp.info.url : ""
                    videoOutput: videoLoader.item
                    audioOutput: AudioOutput { volume: 0.8; muted: insp.muted }
                    loops: insp.looping ? MediaPlayer.Infinite : MediaPlayer.Once
                    property bool primed: false
                    onSourceChanged: primed = false
                    onMediaStatusChanged: {
                        // Load the first frame once per file, without playing (re-entering here would loop).
                        if (mediaStatus === MediaPlayer.LoadedMedia && !primed) { primed = true; mp.play(); mp.pause() }
                    }
                }
            }

            // Stage: poster (frame, cover or waveform) or the live video, with a play badge when idle.
            Item {
                id: stage
                anchors { left: parent.left; right: parent.right; top: parent.top }
                // Fit the picture: video by its aspect ratio, art/waveform by the poster's, never taller than ~55%.
                readonly property real ratio: {
                    if (media.isVideo && insp.info && insp.info.dims) return insp.info.dims[1] / insp.info.dims[0]
                    if (poster.status === Image.Ready && poster.implicitWidth > 0) return poster.implicitHeight / poster.implicitWidth
                    return media.isVideo ? 0.5625 : 0.3
                }
                height: Math.max(Math.round(60 * Style.zoom), Math.min(Math.round(width * ratio), Math.round(media.height * 0.55)))
                Behavior on height { NumberAnimation { duration: Style.anim(120); easing.type: Easing.OutCubic } }
                Rectangle { anchors.fill: parent; radius: 6; color: Style.bgDarker }
                Image {
                    id: poster
                    anchors { fill: parent; margins: 2 }
                    visible: insp.info !== null && insp.info.poster !== undefined && !media.showVideo
                    source: insp.info && insp.info.poster ? insp.info.poster : ""
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                }
                // Made with the player, on first use: the first multimedia object, whichever it is, loads the backend.
                Loader {
                    id: videoLoader
                    anchors { fill: parent; margins: 2 }
                    active: insp.mediaUsed
                    visible: media.showVideo
                    sourceComponent: VideoOutput { fillMode: VideoOutput.PreserveAspectFit }
                }
                // Audio without art: a big note glyph.
                Text {
                    anchors.centerIn: parent
                    visible: !poster.visible && !media.showVideo
                    text: Style.pick("\uf001", "♪")
                    color: Style.fgDim
                    font { family: Style.font; pixelSize: Style.fontSize * 4 }
                }
                // Play badge while paused.
                Rectangle {
                    anchors.centerIn: parent
                    visible: !media.playing
                    width: Math.round(46 * Style.zoom); height: width; radius: width / 2
                    color: Style.alpha(Style.bgDarker, 0.72)
                    border { width: 1; color: Style.alpha(Style.fg, 0.4) }
                    Text {
                        anchors { centerIn: parent; horizontalCenterOffset: 2 }
                        text: Style.pick("\uf04b", "▶")
                        color: Style.fg
                        font { family: Style.font; pixelSize: Style.fontSize + 2 }
                    }
                }
                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    // Hovering a paused video scrubs through it.
                    onPositionChanged: (m) => {
                        if (media.isVideo && media.duration > 0 && !media.playing) {
                            media.scrubbed = true
                            player.position = Math.max(0, Math.min(1, m.x / width)) * media.duration
                        }
                    }
                    onClicked: insp.togglePlayback()
                }
            }

            // Seek bar: click or drag.
            Item {
                id: seek
                anchors { left: parent.left; right: parent.right; top: stage.bottom; topMargin: Style.pad }
                height: Math.round(20 * Style.zoom)
                readonly property real fraction: media.duration > 0 ? Math.min(1, media.position / media.duration) : 0
                Rectangle {
                    id: track
                    anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter }
                    height: 4; radius: 2; color: Style.surface
                    Rectangle { width: parent.width * seek.fraction; height: parent.height; radius: 2; color: Style.accent }
                }
                Rectangle {
                    x: Math.max(0, track.width * seek.fraction - width / 2)
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.round(12 * Style.zoom); height: width; radius: width / 2
                    color: Style.fg
                    visible: media.duration > 0
                }
                MouseArea {
                    anchors { fill: parent; topMargin: -4; bottomMargin: -4 }
                    cursorShape: Qt.PointingHandCursor
                    function seekTo(x) {
                        if (media.duration <= 0) return
                        media.scrubbed = true
                        player.position = Math.max(0, Math.min(1, x / width)) * media.duration
                    }
                    onPressed: (m) => seekTo(m.x)
                    onPositionChanged: (m) => { if (pressed) seekTo(m.x) }
                }
            }

            // Transport controls.
            Row {
                id: controls
                anchors { left: parent.left; top: seek.bottom; topMargin: Style.pad / 2 }
                spacing: 2
                IconButton { glyph: Style.pick("\uf04a", "⏮"); tip: "back 10 s"; onClicked: insp.seekBy(-10000) }
                IconButton {
                    glyph: media.playing ? Style.pick("\uf04c", "⏸") : Style.pick("\uf04b", "▶")
                    tip: "play / pause"
                    active: media.playing
                    onClicked: insp.togglePlayback()
                }
                IconButton { glyph: Style.pick("\uf04e", "⏭"); tip: "forward 10 s"; onClicked: insp.seekBy(10000) }
                IconButton { glyph: Style.pick("\uf01e", "⟲"); tip: "loop"; active: insp.looping; onClicked: insp.toggleLoop() }
                IconButton {
                    glyph: insp.muted ? Style.pick("\uf026", "🔇") : Style.pick("\uf028", "🔊")
                    tip: "mute"; active: insp.muted; onClicked: insp.toggleMute()
                }
            }
            Text {
                anchors { right: parent.right; verticalCenter: controls.verticalCenter }
                color: Style.fgDim
                font { family: Style.font; pixelSize: Style.smallFont }
                text: media.fmt(media.position) + " / " + media.fmt(media.duration)
            }

            // Stream info, then the hotkeys that drive the player.
            Text {
                id: mediaInfo
                anchors { left: parent.left; right: parent.right; top: controls.bottom; topMargin: Style.pad }
                wrapMode: Text.WrapAnywhere
                maximumLineCount: 7
                elide: Text.ElideRight
                color: Style.fgLight
                font { family: Style.font; pixelSize: Style.smallFont }
                text: media.visible ? insp.info.info.join("\n") : ""
            }
            Rectangle {
                anchors { left: parent.left; right: parent.right; top: mediaInfo.bottom; topMargin: Style.pad * 1.5 }
                height: hotkeys.implicitHeight + Style.pad * 1.6
                radius: 5
                color: Style.alpha(Style.surface, 0.5)
                Text {
                    id: hotkeys
                    anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; margins: Style.pad }
                    wrapMode: Text.Wrap
                    textFormat: Text.RichText
                    color: Style.fgDim
                    font { family: Style.font; pixelSize: Style.smallFont }
                    text: "<font color=\"" + Style.fgLight + "\">keys</font>  " + (insp.hotkeyNote !== "" ? insp.hotkeyNote : "click to play · drag the bar to seek")
                          + "<br><font color=\"" + Style.fgLight + "\">mouse</font>  click the picture to play or pause · hover a paused video to scrub"
                }
            }
        }

        // Markdown rendered as rich text.
        Flickable {
            anchors.fill: parent
            visible: insp.info && insp.info.kind === "markdown"
            contentWidth: width
            contentHeight: mdText.implicitHeight
            boundsBehavior: Flickable.StopAtBounds
            Text {
                id: mdText
                width: parent.width
                textFormat: Text.MarkdownText
                wrapMode: Text.Wrap
                text: insp.info && insp.info.kind === "markdown" ? insp.info.text : ""
                color: Style.fg
                linkColor: Style.accent
                font { family: Style.font; pixelSize: Style.smallFont + 1 }
            }
        }

        // Fonts: the family name and a few sample lines drawn with the font itself.
        Item {
            id: fontBox
            anchors.fill: parent
            visible: insp.info && insp.info.kind === "font"
            FontLoader { id: loadedFont; source: insp.info && insp.info.kind === "font" ? insp.info.url : "" }
            Column {
                width: parent.width
                spacing: Style.pad
                Text {
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: loadedFont.name
                    color: Style.accent
                    font { family: Style.font; pixelSize: Style.fontSize; bold: true }
                }
                Repeater {
                    model: [["The quick brown fox jumps over the lazy dog", 22], ["ABCDEFGHIJKLMNOPQRSTUVWXYZ", 18],
                            ["abcdefghijklmnopqrstuvwxyz", 18], ["0123456789  !?&@#$%(){}[]", 18], ["Sphinx of black quartz, judge my vow", 14]]
                    delegate: Text {
                        required property var modelData
                        width: fontBox.width
                        wrapMode: Text.Wrap
                        text: modelData[0]
                        color: Style.fg
                        font { family: loadedFont.name; pixelSize: Math.round(modelData[1] * Style.zoom) }
                    }
                }
                Text {
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: insp.info && insp.info.info ? insp.info.info.join("\n") : ""
                    color: Style.fgDim
                    font { family: Style.font; pixelSize: Style.smallFont }
                }
            }
        }

        Text {
            anchors.centerIn: parent
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            visible: insp.info && (insp.info.kind === "binary" || insp.info.kind === "none")
            color: Style.fgDim
            font { family: Style.font; pixelSize: Style.smallFont }
            text: insp.info ? (insp.info.message ?? "") : ""
        }

        Text {
            anchors.centerIn: parent
            visible: insp.info === null
            color: Style.fgDim
            font { family: Style.font; pixelSize: Style.smallFont }
            text: insp.path !== "" ? "Loading preview…" : "Nothing selected"
        }
    }

}
