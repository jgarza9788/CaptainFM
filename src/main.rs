mod backend;
mod batch;
mod cli;
mod dbus;
mod devices;
mod dir_model;
mod fuzzy;
mod index;
mod listing;
mod openwith;
mod ops;
mod plugins;
mod preview;
mod preview_extra;
mod state;
#[cfg(test)]
mod testutil;
mod theme;
mod thumbnails;
mod watch;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let cli::Action::Exit(code) = cli::handle(&args) {
        std::process::exit(code);
    }

    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/CaptainFM/qml/Main.qml"));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
