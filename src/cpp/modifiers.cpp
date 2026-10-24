#include <QtGui/QGuiApplication>

// Keys held right now (Qt::KeyboardModifier flags), asked of the window system, not Qt's cache of key
// events: that cache goes stale while a drag is under way, when QML only sees the pointer.
extern "C" int cfm_held_modifiers() {
    return int(QGuiApplication::queryKeyboardModifiers());
}
