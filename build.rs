use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    let qml_files = [
        QmlFile::from("qml/Style.qml").singleton(true),
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
    ];
    CxxQtBuilder::new_qml_module(QmlModule::new("CaptainFM").qml_files(qml_files))
        .files(["src/dir_model.rs", "src/theme.rs", "src/backend.rs"])
        .build();
}
