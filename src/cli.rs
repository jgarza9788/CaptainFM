//! Command-line handling that runs before Qt starts: `--help`, `--version`, `--install`,
//! `--default [off]`. Everything else is forwarded to the QML app.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::state::{data_dir, home};

const DESKTOP_ID: &str = "captainfm.desktop";
const ICON_SVG: &str = include_str!("../packaging/captainfm.svg");

const USAGE: &str = "usage: captainfm [--select <path>] [path...]
       captainfm --tab <path...>  open folders as tabs in the running CaptainFM (starts one if none)
       captainfm --install        install the desktop entry and icon for the current user
       captainfm --default [off]  make CaptainFM the folder handler (or restore the previous one)
       captainfm --install-portal [off]
                                  use CaptainFM as the file picker (xdg-desktop-portal FileChooser)
       captainfm --install-dbus   let other apps open folders in CaptainFM (FileManager1)
       captainfm --version

Each path opens in its own tab; --select opens the parent folder with the item highlighted.";

pub enum Action {
    /// Start the GUI.
    Run,
    /// Exit with this status after printing.
    Exit(i32),
}

pub fn handle(args: &[String]) -> Action {
    let first = args.first().map(String::as_str);
    match first {
        Some("--help" | "-h") => {
            println!("{USAGE}");
            Action::Exit(0)
        }
        Some("--version" | "-V") => {
            println!("captainfm {}", env!("CARGO_PKG_VERSION"));
            Action::Exit(0)
        }
        Some("--install") => Action::Exit(report(install())),
        Some("--tab") => {
            let paths: Vec<String> = args[1..]
                .iter()
                .map(|p| std::fs::canonicalize(p).map_or_else(|_| p.clone(), |c| c.to_string_lossy().into_owned()))
                .collect();
            if paths.is_empty() {
                Action::Run
            } else if crate::dbus::forward_to_running(&paths) {
                Action::Exit(0)
            } else {
                // Nothing running: start normally, the paths become tabs.
                Action::Run
            }
        }
        Some("--portal") => Action::Exit(crate::dbus::run_portal()),
        Some("--install-portal") => {
            let off = args.get(1).map(String::as_str) == Some("off");
            Action::Exit(report(set_portal(!off)))
        }
        Some("--install-dbus") => Action::Exit(report(install_dbus())),
        Some("--default") => {
            let off = args.get(1).map(String::as_str) == Some("off");
            Action::Exit(report(if off { restore_default() } else { make_default() }))
        }
        _ => Action::Run,
    }
}

fn report(result: Result<String, String>) -> i32 {
    match result {
        Ok(msg) => {
            println!("{msg}");
            0
        }
        Err(e) => {
            eprintln!("captainfm: {e}");
            1
        }
    }
}

fn applications_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".local/share"))
        .join("applications")
}

fn icons_dir() -> PathBuf {
    applications_dir().parent().unwrap().join("icons/hicolor/scalable/apps")
}

fn desktop_entry(exe: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=CaptainFM\n\
         GenericName=File Manager\n\
         Comment=Fast, keyboard-first file manager for Omarchy\n\
         Exec={exe} %F\n\
         Icon=captainfm\n\
         Terminal=false\n\
         Categories=System;FileTools;FileManager;\n\
         MimeType=inode/directory;\n\
         Keywords=files;folder;manager;browser;captainfm;cfm;\n\
         StartupWMClass=captainfm\n"
    )
}

fn install() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let apps = applications_dir();
    let icons = icons_dir();
    fs::create_dir_all(&apps).map_err(|e| e.to_string())?;
    fs::create_dir_all(&icons).map_err(|e| e.to_string())?;
    fs::write(apps.join(DESKTOP_ID), desktop_entry(&exe.to_string_lossy())).map_err(|e| e.to_string())?;
    fs::write(icons.join("captainfm.svg"), ICON_SVG).map_err(|e| e.to_string())?;
    // Best effort: refresh the desktop database so launchers notice the new entry.
    let _ = Command::new("update-desktop-database").arg(&apps).status();
    Ok(format!("Installed {} and its icon", apps.join(DESKTOP_ID).display()))
}

fn write_file(path: &std::path::Path, text: &str) -> Result<(), String> {
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| e.to_string())
}

/// Register CaptainFM as an xdg-desktop-portal backend and make it the file chooser.
fn install_portal() -> Result<String, String> {
    use crate::dbus::{PORTAL_NAME, portal_definition, portal_file, portal_service_file, service_definition};
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    write_file(&portal_file(), &portal_definition())?;
    write_file(
        &portal_service_file(),
        &service_definition(PORTAL_NAME, &format!("{} --portal", exe.display())),
    )?;
    let conf = crate::dbus::portals_conf_path();
    let old = fs::read_to_string(&conf).unwrap_or_default();
    write_file(&conf, &crate::dbus::with_file_chooser(&old, true))?;
    Ok(format!(
        "CaptainFM is now the file chooser (set in {}). Undo with: captainfm --install-portal off",
        conf.display()
    ))
}

fn uninstall_portal() -> Result<String, String> {
    let _ = fs::remove_file(crate::dbus::portal_file());
    let _ = fs::remove_file(crate::dbus::portal_service_file());
    let conf = crate::dbus::portals_conf_path();
    if let Ok(old) = fs::read_to_string(&conf) {
        write_file(&conf, &crate::dbus::with_file_chooser(&old, false))?;
    }
    Ok("CaptainFM is no longer the file chooser.".into())
}

/// Install or remove the portal, then restart xdg-desktop-portal so the change applies at once.
pub fn set_portal(on: bool) -> Result<String, String> {
    let msg = if on { install_portal() } else { uninstall_portal() }?;
    // CFM_NO_RESTART=1 leaves the running portal alone (packaging and tests on a private bus).
    if std::env::var_os("CFM_NO_RESTART").is_none() {
        let _ = Command::new("systemctl").args(["--user", "restart", "xdg-desktop-portal"]).status();
    }
    Ok(msg)
}

/// Whether CaptainFM is currently configured as the file chooser.
pub fn portal_installed() -> bool {
    crate::dbus::portal_file().exists()
        && fs::read_to_string(crate::dbus::portals_conf_path()).is_ok_and(|c| crate::dbus::file_chooser_preferred(&c))
}

fn install_dbus() -> Result<String, String> {
    use crate::dbus::{FILE_MANAGER_NAME, file_manager_service_file, service_definition};
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = file_manager_service_file();
    write_file(&path, &service_definition(FILE_MANAGER_NAME, &exe.to_string_lossy()))?;
    Ok(format!(
        "Installed {}: \"Show in folder\" requests will start CaptainFM when no file manager owns org.freedesktop.FileManager1.",
        path.display()
    ))
}

fn xdg_mime(args: &[&str]) -> Option<String> {
    Command::new("xdg-mime")
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
}

/// Whether CaptainFM is currently the `inode/directory` handler.
pub fn is_default_handler() -> bool {
    xdg_mime(&["query", "default", "inode/directory"]).is_some_and(|d| d == DESKTOP_ID)
}

fn previous_file() -> PathBuf {
    data_dir().join("previous-directory-handler")
}

fn make_default() -> Result<String, String> {
    install()?;
    let current = xdg_mime(&["query", "default", "inode/directory"]).unwrap_or_default();
    if current != DESKTOP_ID && !current.is_empty() {
        fs::create_dir_all(data_dir()).map_err(|e| e.to_string())?;
        fs::write(previous_file(), &current).map_err(|e| e.to_string())?;
    }
    xdg_mime(&["default", DESKTOP_ID, "inode/directory"]);
    Ok(format!(
        "CaptainFM now opens folders (was: {}). Undo with: captainfm --default off",
        if current.is_empty() { "unset" } else { &current }
    ))
}

fn restore_default() -> Result<String, String> {
    let previous = fs::read_to_string(previous_file()).map_err(|_| "no previous handler was recorded".to_owned())?;
    xdg_mime(&["default", previous.trim(), "inode/directory"]);
    let _ = fs::remove_file(previous_file());
    Ok(format!("Folder handler restored to {}", previous.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_is_a_folder_handler() {
        let e = desktop_entry("/usr/bin/captainfm");
        assert!(e.contains("Exec=/usr/bin/captainfm %F"));
        assert!(e.contains("MimeType=inode/directory;"));
    }

    #[test]
    fn plain_paths_start_the_gui() {
        assert!(matches!(handle(&["/tmp".to_owned()]), Action::Run));
        assert!(matches!(handle(&[]), Action::Run));
        assert!(matches!(handle(&["--version".to_owned()]), Action::Exit(0)));
    }
}

#[cfg(test)]
mod install_tests {
    use super::*;
    use crate::testutil::Sandbox;

    #[test]
    fn install_writes_a_desktop_entry_and_icon() {
        let sb = Sandbox::new("cli-install");
        let msg = install().unwrap();
        assert!(msg.contains("captainfm.desktop"));
        let entry = fs::read_to_string(sb.root.join(".local/share/applications/captainfm.desktop")).unwrap();
        assert!(entry.contains("MimeType=inode/directory;") && entry.contains("StartupWMClass=captainfm"));
        let icon = fs::read_to_string(sb.root.join(".local/share/icons/hicolor/scalable/apps/captainfm.svg")).unwrap();
        assert!(icon.starts_with("<svg"));
    }

    #[test]
    fn portal_install_and_removal() {
        let sb = Sandbox::new("cli-portal");
        let msg = install_portal().unwrap();
        assert!(msg.contains("file chooser"));
        assert!(crate::dbus::portal_file().exists() && crate::dbus::portal_service_file().exists());
        assert!(portal_installed());
        let service = fs::read_to_string(crate::dbus::portal_service_file()).unwrap();
        assert!(service.contains("--portal"));
        uninstall_portal().unwrap();
        assert!(!crate::dbus::portal_file().exists() && !crate::dbus::portal_service_file().exists());
        assert!(!portal_installed());
        drop(sb);
    }

    #[test]
    fn dbus_activation_file_is_written() {
        let _sb = Sandbox::new("cli-dbus");
        install_dbus().unwrap();
        let text = fs::read_to_string(crate::dbus::file_manager_service_file()).unwrap();
        assert!(text.contains("Name=org.freedesktop.FileManager1"));
    }

    #[test]
    fn restoring_without_a_record_is_an_error() {
        let _sb = Sandbox::new("cli-restore");
        assert!(restore_default().is_err());
    }

    #[test]
    fn flags_are_recognised() {
        assert!(matches!(handle(&["--help".to_owned()]), Action::Exit(0)));
        assert!(matches!(handle(&["--select".to_owned(), "/tmp/x".to_owned()]), Action::Run));
    }
}
