import QtQuick
import CaptainFM

// A turning arc: the UI is alive while a server keeps us waiting.
Canvas {
    width: Math.round(Style.fontSize * 1.1); height: width
    property color tint: Style.yellow
    onTintChanged: requestPaint()
    onPaint: {
        const ctx = getContext("2d")
        ctx.reset()
        ctx.lineWidth = Math.max(1.5, width / 7)
        ctx.lineCap = "round"
        ctx.strokeStyle = tint
        ctx.beginPath()
        ctx.arc(width / 2, height / 2, width / 2 - ctx.lineWidth, 0, Math.PI * 1.5)
        ctx.stroke()
    }
    RotationAnimator on rotation {
        running: parent.visible; loops: Animation.Infinite
        from: 0; to: 360; duration: 900
    }
}
