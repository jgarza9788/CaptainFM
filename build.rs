use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    let qml_files = [
        QmlFile::from("qml/Style.qml").singleton(true),
        QmlFile::from("qml/Prefs.qml").singleton(true),
        QmlFile::from("qml/Dnd.qml").singleton(true),
        QmlFile::from("qml/DragGhost.qml"),
        QmlFile::from("qml/HoverOpen.qml"),
        QmlFile::from("qml/DropWatch.qml"),
        QmlFile::from("qml/Main.qml"),
        QmlFile::from("qml/Pane.qml"),
        QmlFile::from("qml/TabPage.qml"),
        QmlFile::from("qml/Inspector.qml"),
        QmlFile::from("qml/PromptBar.qml"),
        QmlFile::from("qml/Picker.qml"),
        QmlFile::from("qml/JumpBar.qml"),
        QmlFile::from("qml/LeaderHints.qml"),
        QmlFile::from("qml/Toast.qml"),
        QmlFile::from("qml/PickBar.qml"),
        QmlFile::from("qml/MillerColumn.qml"),
        QmlFile::from("qml/PluginApi.qml"),
        QmlFile::from("qml/Breadcrumbs.qml"),
        QmlFile::from("qml/Sidebar.qml"),
        QmlFile::from("qml/KeyEngine.qml"),
        QmlFile::from("qml/Commands.qml"),
        QmlFile::from("qml/BatchRename.qml"),
        QmlFile::from("qml/Settings.qml"),
        QmlFile::from("qml/ConnectDialog.qml"),
        QmlFile::from("qml/ConflictDialog.qml"),
        QmlFile::from("qml/TransferPanel.qml"),
    ];
    // SAFETY: the closure only adds one C++ file of ours to the build.
    unsafe {
        CxxQtBuilder::new_qml_module(QmlModule::new("CaptainFM").qml_files(qml_files))
            .files(["src/dir_model.rs", "src/theme.rs", "src/backend.rs"])
            .cc_builder(|cc| {
                cc.file("src/cpp/modifiers.cpp");
            })
            .build();
    }
    println!("cargo:rerun-if-changed=src/cpp/modifiers.cpp");
}
