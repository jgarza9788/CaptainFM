import QtQuick
import CaptainFM

// Seconds spent waiting, shown after 3 s so a slow server is told apart from a stuck one.
Timer {
    property int seconds: 0
    interval: 1000; repeat: true
    onRunningChanged: seconds = 0
    onTriggered: seconds++
    function suffix() { return seconds >= 3 ? "  " + seconds + " s" : "" }
}
