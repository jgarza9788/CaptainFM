import QtQuick
import CaptainFM

// Bottom bar shown when CaptainFM runs as a file picker (xdg-desktop-portal FileChooser).
Rectangle {
    id: bar
    height: Math.round(40 * Style.zoom)
    color: Style.bgDark

    property var req: ({})
    property int filterIndex: 0
    readonly property alias name: nameInput.text
    readonly property string acceptText: (req.acceptLabel ?? "") !== "" ? req.acceptLabel
                                         : (req.save ? "Save" : (req.directory ? "Select folder" : "Open"))
    readonly property var filters: req.filters ?? []

    signal accepted()
    signal cancelled()
    signal filterCycled()

    function setName(n) { nameInput.text = n }
    function focusName() { nameInput.forceActiveFocus() }

    Rectangle { anchors { left: parent.left; right: parent.right; top: parent.top } height: 1; color: Style.surface }

    Text {
        id: titleText
        anchors { left: parent.left; leftMargin: Style.pad * 1.5; verticalCenter: parent.verticalCenter }
        text: bar.req.title ?? ""
        color: Style.accent
        font { family: Style.font; pixelSize: Style.fontSize; bold: true }
        width: Math.min(implicitWidth, parent.width * 0.25)
        elide: Text.ElideRight
    }

    Rectangle {
        visible: bar.req.save === true
        anchors { left: titleText.right; leftMargin: Style.pad; right: filterChip.visible ? filterChip.left : buttons.left
                  rightMargin: Style.pad; verticalCenter: parent.verticalCenter }
        height: parent.height - 12
        radius: 4
        color: Style.bgDarker
        border { width: 1; color: nameInput.activeFocus ? Style.accent : Style.surface }
        TextInput {
            id: nameInput
            anchors { fill: parent; leftMargin: Style.pad; rightMargin: Style.pad }
            verticalAlignment: TextInput.AlignVCenter
            clip: true
            color: Style.fg
            selectionColor: Style.accent
            selectedTextColor: Style.bgDarker
            font { family: Style.font; pixelSize: Style.fontSize }
            onAccepted: bar.accepted()
            Keys.onReturnPressed: (e) => { e.accepted = true; bar.accepted() }
            Keys.onEnterPressed: (e) => { e.accepted = true; bar.accepted() }
            Keys.onEscapePressed: bar.cancelled()
        }
    }

    Rectangle {
        id: filterChip
        visible: bar.filters.length > 0
        anchors { right: buttons.left; rightMargin: Style.pad; verticalCenter: parent.verticalCenter }
        height: Math.round(26 * Style.zoom)
        width: chipText.implicitWidth + Style.pad * 2
        radius: 13
        color: Style.surface
        Text {
            id: chipText
            anchors.centerIn: parent
            text: bar.filters.length > 0 ? bar.filters[bar.filterIndex].name + "  ▾" : ""
            color: Style.fgLight
            font { family: Style.font; pixelSize: Style.smallFont }
        }
        MouseArea { anchors.fill: parent; onClicked: bar.filterCycled() }
    }

    Row {
        id: buttons
        anchors { right: parent.right; rightMargin: Style.pad; verticalCenter: parent.verticalCenter }
        spacing: Style.pad
        component Button: Rectangle {
            id: btn
            property alias text: label.text
            property bool primary: false
            signal clicked()
            height: Math.round(28 * Style.zoom)
            width: label.implicitWidth + Style.pad * 3
            radius: 5
            color: primary ? Style.accent : Style.surface
            Text {
                id: label
                anchors.centerIn: parent
                color: btn.primary ? Style.bgDarker : Style.fg
                font { family: Style.font; pixelSize: Style.fontSize; bold: btn.primary }
            }
            MouseArea { anchors.fill: parent; onClicked: btn.clicked() }
        }
        Button { text: "Cancel"; onClicked: bar.cancelled() }
        Button { text: bar.acceptText; primary: true; onClicked: bar.accepted() }
    }
}
