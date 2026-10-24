import QtQuick
import CaptainFM

// Put inside a DropArea (`area`): while a drag is over it, watch the Shift, Ctrl and Alt keys and, when they
// change, hand back the new drop hint so the label follows the keys without the pointer moving.
Item {
    id: watch
    property var area
    property string destination: ""
    property string name: ""
    property bool trash: false
    signal refreshed(var hint)

    Timer {
        running: watch.area && watch.area.containsDrag
        interval: 60
        repeat: true
        onTriggered: Dnd.sample()
    }
    Connections {
        target: Dnd
        function onModsChanged() {
            if (watch.area && watch.area.containsDrag) watch.refreshed(Dnd.again(watch.destination, watch.name, watch.trash))
        }
    }
}
