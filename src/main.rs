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
mod remotes;
mod state;
#[cfg(test)]
mod testutil;
mod theme;
mod thumbnails;
mod transfer;
mod watch;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

/// When the program started, for the `CFM_DEBUG` startup timings.
static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn main() {
    START.get_or_init(std::time::Instant::now);
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let cli::Action::Exit(code) = cli::handle(&args) {
        std::process::exit(code);
    }

    std::thread::spawn(preview::warm_up);
    let mut app = QGuiApplication::new();
    let debug = std::env::var_os("CFM_DEBUG").is_some();
    let lap = |what: &str| {
        if debug {
            eprintln!("[timing] {what} {} ms after start", START.get().map_or(0, |t| t.elapsed().as_millis()));
        }
    };
    lap("Qt application created");
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/CaptainFM/qml/Main.qml"));
    }
    lap("QML loaded");

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
