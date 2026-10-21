import QtQuick
import CaptainFM

// Clickable path: "~/Projects/" dim with the last segment bold. The end of a long path stays visible.
Item {
    id: crumbs

    property string path: ""
    property string home: Backend.home()
    property bool strong: true            // false: draw everything dim (inactive pane)
    property string trailing: ""          // extra text after the path (search query)
    property int pixelSize: Style.fontSize
    signal navigate(string target)

    clip: true
    implicitHeight: row.implicitHeight

    readonly property var parts: {
        const p = path
        if (p === "") return []
        const inHome = home !== "" && (p === home || p.indexOf(home + "/") === 0)
        const base = inHome ? home : "/"
        const out = [{ label: inHome ? "~" : "/", path: base }]
        const rest = p.substring(base === "/" ? 1 : base.length).split("/").filter(x => x !== "")
        let acc = base === "/" ? "" : base
        for (const seg of rest) { acc += "/" + seg; out.push({ label: seg, path: acc }) }
        return out
    }

    Row {
        id: row
        // Short paths sit at the left; long ones scroll so the end stays visible.
        x: Math.min(0, crumbs.width - width)
        height: parent.height
        spacing: 0
        Repeater {
            model: crumbs.parts
            delegate: Row {
                id: item
                required property int index
                required property var modelData
                readonly property bool last: index === crumbs.parts.length - 1 && crumbs.trailing === ""
                height: crumbs.height
                Text {
                    visible: item.index > 1 || (item.index === 1 && crumbs.parts[0].label !== "/")
                    anchors.verticalCenter: parent.verticalCenter
                    text: "/"
                    color: Style.fgDim
                    font { family: Style.font; pixelSize: crumbs.pixelSize }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: item.modelData.label
                    color: seg.containsMouse ? Style.accent : (item.last && crumbs.strong ? Style.fg : Style.fgDim)
                    font { family: Style.font; pixelSize: crumbs.pixelSize; bold: item.last }
                    MouseArea {
                        id: seg
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: crumbs.navigate(item.modelData.path)
                    }
                }
                Text {
                    visible: item.index === 0 && crumbs.parts.length > 1 && item.modelData.label === "/"
                    text: ""
                }
            }
        }
        Text {
            visible: crumbs.trailing !== ""
            anchors.verticalCenter: parent.verticalCenter
            text: "  ›  "
            color: Style.fgDim
            font { family: Style.font; pixelSize: crumbs.pixelSize }
        }
        Text {
            visible: crumbs.trailing !== ""
            anchors.verticalCenter: parent.verticalCenter
            text: crumbs.trailing
            color: Style.fg
            font { family: Style.font; pixelSize: crumbs.pixelSize; bold: true }
        }
    }
}
