extern crate native_windows_gui as nwg;
use nwg::NativeUi;
use mslnk::ShellLink;
use parselnk::Lnk;
use chrono::Local;
use std::ffi::OsString;
use std::env;
use std::time::SystemTime;
use std::fs::{self, File};
use std::io::{self, Read, Write, BufReader};
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use sysinfo::{System, SystemExt};
use winapi::um::knownfolders::FOLDERID_LocalAppData;
use winapi::um::shlobj::CSIDL_STARTMENU;
use winapi::um::shlobj::SHGetSpecialFolderPathW;
use winapi::um::shlobj::{SHGetKnownFolderPath};
use winapi::um::winnt::PWSTR;
use winapi::shared::winerror::S_OK;
//use windows::Win32::UI::WindowsAndMessaging::{PostQuitMessage};

mod zip_utils;

use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static APP_NAME: Lazy<Mutex<Option<Box<str>>>> = Lazy::new(|| Mutex::new(None));
pub static DEBUG: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));
pub static EXE_PATH_TO_RUN: Lazy<Mutex<Option<PathBuf>>> = Lazy::new(|| Mutex::new(None));

#[derive(Default)]
pub struct FlexBoxApp {
    window: nwg::Window,
    layout: nwg::FlexboxLayout,
    listview: nwg::ListView,
    progress_bar: nwg::ProgressBar,
    layout2: nwg::FlexboxLayout,
    button1: nwg::Button,
    button2: nwg::Button,
    button3: nwg::Button,
    spacer1: nwg::Frame, 
    spacer2: nwg::Frame, 
}

impl FlexBoxApp {
    fn exit(&self) {
        nwg::stop_thread_dispatch();
    }
}

mod flexbox_app_ui {
    use native_windows_gui as nwg;
    use super::*;
    use std::rc::Rc;
    use std::cell::RefCell;
    use std::ops::Deref;
    use std::process::Command;

    pub struct FlexBoxAppUi {
        inner: Rc<FlexBoxApp>,
        default_handler: RefCell<Option<nwg::EventHandler>>
    }

    impl nwg::NativeUi<FlexBoxAppUi> for FlexBoxApp {
        fn build_ui(mut data: FlexBoxApp) -> Result<FlexBoxAppUi, 
                nwg::NwgError> {
            use nwg::Event as E;
            
            // Controls
            nwg::Window::builder()
                .size((700, 500))
                .position((300, 300))
                .title("App Installer")
                .build(&mut data.window)?;

            nwg::ListView::builder()
                .parent(&data.window)
                .focus(true)
                .list_style(nwg::ListViewStyle::Detailed)
                .ex_flags(nwg::ListViewExFlags::GRID | 
                        nwg::ListViewExFlags::FULL_ROW_SELECT)
                .item_count(15)
                .build(&mut data.listview)?;

            nwg::ProgressBar::builder()
                .parent(&data.window)
                .build(&mut data.progress_bar)?;

            nwg::Button::builder()
                .text("Close")
                .parent(&data.window)
                .focus(true)
                .build(&mut data.button1)?;

            nwg::Button::builder()
                .text("Copy to Clipboard")
                .parent(&data.window)
                .focus(true)
                .build(&mut data.button2)?;

            nwg::Button::builder()
                .text("Cancel")
                .parent(&data.window)
                .focus(true)
                .build(&mut data.button3)?;

            nwg::Frame::builder()
                .parent(&data.window)
                .flags(nwg::FrameFlags::VISIBLE)
                .build(&mut data.spacer1)?;

            nwg::Frame::builder()
                .parent(&data.window)
                .flags(nwg::FrameFlags::VISIBLE)
                .build(&mut data.spacer2)?;

            // Wrap-up
            let ui = FlexBoxAppUi {
                inner:  Rc::new(data),
                default_handler: Default::default(),
            };

            // Events
            let evt_ui = Rc::downgrade(&ui.inner);
            let handle_events = move |evt, _evt_data, handle| {
                if let Some(evt_ui) = evt_ui.upgrade() {
                    match evt {
                        E::OnWindowClose => {
                            if &handle == &evt_ui.window {
                                FlexBoxApp::exit(&evt_ui);
                            }
                        },
                        E::OnButtonClick => {
                            if &handle == &evt_ui.button1 {
                                if let Some(path) = EXE_PATH_TO_RUN.lock().unwrap().take() {
                                    Command::new(path)
                                        .spawn()
                                        .expect("Failed to run executable");
                                }
                                FlexBoxApp::exit(&evt_ui);
                            } else if &handle == &evt_ui.button2 {
                                let mut text = String::new();
                                for i in 0..evt_ui.listview.len() {
                                    if let Some(item) = evt_ui.listview.item(i as usize, 0, 256) {
                                        text.push_str(&item.text);
                                        text.push_str("\t");
                                    }
                                    if let Some(item) = evt_ui.listview.item(i as usize, 1, 256) {
                                        text.push_str(&item.text);
                                        text.push_str("\t");
                                    }
                                    if let Some(item) = evt_ui.listview.item(i as usize, 2, 1024) {
                                        text.push_str(&item.text);
                                        text.push_str("\r\n");
                                    }
                                }
                                nwg::Clipboard::set_data_text(&evt_ui.window.handle, &text);
                            } else if &handle == &evt_ui.button3 {
                                FlexBoxApp::exit(&evt_ui);
                            }
                        },
                        E::OnResize => {
                            if &handle == &evt_ui.window {
                                let (w, _) = evt_ui.listview.size();
                                evt_ui.listview.set_column_width(2, 
                                        (w - 200) as isize);
                            }
                        },
                        _ => {}
                    }
                }
            };

           *ui.default_handler.borrow_mut() = Some(
                    nwg::full_bind_event_handler(&ui.window.handle, 
                            handle_events));

            ui.listview.insert_column("TYPE");
            ui.listview.set_column_width(0, 100);
            ui.listview.set_column_sort_arrow(0, 
                    Some(nwg::ListViewColumnSortArrow::Down));

            ui.listview.insert_column("TIME");
            ui.listview.set_column_width(1, 100);
            ui.listview.set_column_sort_arrow(1, 
                    Some(nwg::ListViewColumnSortArrow::Down));

            ui.listview.insert_column("MESSAGE");
            // Set this to window width - 20
            ui.listview.set_column_width(2, 480);
            ui.listview.set_column_sort_arrow(2, 
                    Some(nwg::ListViewColumnSortArrow::Down));

            ui.listview.set_headers_enabled(true);

            // Layout
            use nwg::stretch::{geometry::Size, style::{Dimension as D, 
                    FlexDirection}};

            nwg::FlexboxLayout::builder()
                .parent(&ui.window)
                .flex_direction(FlexDirection::Row)
                .child(&ui.spacer1)
                    .child_flex_grow(2.0)
                    .child_size(Size { width: D::Points(200.0), 
                            height: D::Points(20.0) })
                .child(&ui.button1)
                    .child_size(Size { width: D::Points(200.0), 
                            height: D::Points(40.0) })
                .child(&ui.button2)
                    .child_size(Size { width: D::Points(310.0), 
                            height: D::Points(40.0) })
                .child(&ui.button3)
                    .child_size(Size { width: D::Points(200.0), 
                            height: D::Points(40.0) })
                .child(&ui.spacer2)
                    .child_flex_grow(2.0)
                    .child_size(Size { width: D::Points(200.0), 
                            height: D::Points(20.0) })
                .build_partial(&ui.layout2)?;

            nwg::FlexboxLayout::builder()
                .parent(&ui.window)
                .flex_direction(FlexDirection::Column)
                .child(&ui.listview)
                    .child_flex_grow(2.0)
                    .child_size(Size { width: D::Auto, height: D::Auto })
                .child(&ui.progress_bar)
                    .child_size(Size{width: D::Auto, height: D::Points(20.0) })
                .child_layout(&ui.layout2)
                    .child_size(Size { width: D::Auto, height: D::Auto })
                .build(&ui.layout)?;
            
            return Ok(ui);
        }
    }

    impl Drop for FlexBoxAppUi {
        /// To make sure that everything is freed without issues, the default 
        /// handler must be unbound.
        fn drop(&mut self) {
            let handler = self.default_handler.borrow();
            if handler.is_some() {
                nwg::unbind_event_handler(handler.as_ref().unwrap());
            }
        }
    }

    impl Deref for FlexBoxAppUi {
        type Target = FlexBoxApp;

        fn deref(&self) -> &FlexBoxApp {
            &self.inner
        }
    }
}

fn get_local_appdata(listview: &nwg::ListView) -> Option<PathBuf> {
    let mut path_ptr: PWSTR = std::ptr::null_mut();
    let result = unsafe {
        SHGetKnownFolderPath(
            &FOLDERID_LocalAppData,
            0,
            std::ptr::null_mut(),
            &mut path_ptr
        )
    };
    if result == S_OK {
        let len = unsafe {
            (0..).take_while(|&i| *path_ptr.offset(i) != 0).count()
        };
        let path_slice = unsafe {
            std::slice::from_raw_parts(path_ptr, len)
        };
        let os_string: OsString = OsStringExt::from_wide(path_slice);
        let mut path = PathBuf::from(os_string);
        path.push("Utils");
        if !path.exists() {
            if let Err(e) = fs::create_dir_all(&path) {
                add_message(&listview, "ERROR",
                    &format!("Failed to create directory {:?}: {}", path, e));
                return None;
            }
        }
        Some(path)
    } else {
        None
    }
}

pub fn run_installation(listview: &nwg::ListView, bar: &nwg::ProgressBar, 
        app_name: &str) {
    update_installer(&listview, bar);

    add_message(&listview, "INFO", &format!("Starting installation for {}",
            app_name));

    let process_name = format!("{}.exe", app_name);
    if check_if_running(&process_name) {
        add_message(&listview, "ERROR",
            &format!( "'{}' is running. Please close it and try again.",
                app_name
            )
        );
        return;
    }

    uninstall_application(&listview, app_name);

    if let Some(copied_zip_path) = copy_latest_zip(&listview, &bar, app_name) {
        unzip_file(&listview, &copied_zip_path, app_name);

        if let Some(local_appdata) = get_local_appdata(&listview) {
            let app_dir = local_appdata.join(app_name);
            if let Some(exe_path) = find_executable(&app_dir) {
                add_message(&listview, "DEBUG", 
                        &format!("Found executable at {:?}", exe_path));
                if let Some(exe_str) = exe_path.to_str() {
                    create_shortcut(&listview, exe_str, app_name);
                    *EXE_PATH_TO_RUN.lock().unwrap() = Some(exe_path.clone());
                } else {
                    add_message(&listview, "ERROR",
                        "Executable path contains invalid characters.");
                }
            } else {
                add_message(&listview, "ERROR",
                    &format!("Could not find executable for {}", app_name),
                );
            }
        }

        if let Err(e) = fs::remove_file(&copied_zip_path) {
            add_message(&listview, "ERROR",
                &format!("Failed to delete temporary zip file: {}", e),
            );
        }
    } else {
        add_message(&listview, "ERROR", 
                &format!("Installation failed for {}.", app_name));
    }
    add_message(&listview, "INFO", "Installation process finished.");
}

pub fn add_message(listview: &nwg::ListView, message_type: &str, message: &str) {
    if message_type == "DEBUG" && !*DEBUG.lock().unwrap() {
        return;
    }
    let time_str = Local::now().format("%H:%M:%S").to_string();
    listview.insert_item(message_type);
    let new_index = (listview.len() - 1) as i32;
    listview.insert_item(nwg::InsertListViewItem { 
        index: Some(new_index),
        column_index: 1,
        text: Some(time_str.into()),
        image: None
    });
    listview.insert_item(nwg::InsertListViewItem { 
        index: Some(new_index),
        column_index: 2,
        text: Some(message.into()),
        image: None
    });
}

fn update_installer(listview: &nwg::ListView, bar: &nwg::ProgressBar) {
    add_message(&listview, "INFO", "Checking for installer updates...");
    if let Some(local_appdata) = get_local_appdata(&listview) {
        let local_installer_path = local_appdata.join(
                "AppInstaller").join("AppInstaller.exe");
        if !local_installer_path.exists() {
            add_message(&listview, "INFO", 
                    "No local installer found. Downloading...");
            get_installer(&listview, &bar);
            return;
        }

        if let Ok(current_exe) = env::current_exe() {
            if let Ok(local_meta) = fs::metadata(&current_exe) {
                if let Ok(local_time) = local_meta.modified() {
                    perform_installer_update(local_time, current_exe, 
                            &listview, &bar);
                }
            }
        }
    }
}

fn perform_installer_update(local_time: SystemTime, current_exe: PathBuf, 
        listview: &nwg::ListView, bar: &nwg::ProgressBar) {
    let remote_dir = Path::new(r"C:\dev\apps").join(
            "AppInstaller");
    let mut newest_remote_file: Option<(PathBuf, 
            SystemTime)> = None;
    if let Ok(entries) = fs::read_dir(&remote_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension(
                        ).and_then(|s| s.to_str()) == 
                        Some("zip") {
                    if let Ok(metadata) = 
                            fs::metadata(&path) {
                        if let Ok(modified) = 
                                metadata.modified() {
                            if newest_remote_file.is_none() || modified > 
                                    newest_remote_file.as_ref().unwrap().1 {
                                newest_remote_file = Some((path, modified));
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some((_, remote_time)) = newest_remote_file {
        if remote_time > local_time {
            add_message(&listview, "INFO", 
                    "Newer installer found. Updating...");
            let new_name = current_exe.with_extension("AppInstaller.old");
            if let Err(e) = fs::rename(&current_exe, &new_name) {
                add_message(&listview, "ERROR",
                    &format!("Failed to rename old installer: {}", e),
                );
                return;
            }
            get_installer(&listview, &bar);
            add_message(&listview, "INFO", "Installer updated.");
            //unsafe { PostQuitMessage(0); }
        }
    }
}

fn check_if_running(process_name: &str) -> bool {
    let s = System::new_all();
    for _process in s.processes_by_name(process_name) {
        return true;
    }
    false
}

fn uninstall_application(listview: &nwg::ListView, app_name: &str) {
    add_message(&listview, "DEBUG",
        &format!("Attempting to uninstall application: {}", app_name));
    let shortcut_name = add_spaces(app_name);
    if let Some((shortcut_path, target_dir)) = find_shortcut(&shortcut_name) {
        if target_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&target_dir) {
                add_message(&listview, "ERROR",
                    &format!("Failed to delete directory '{:?}': {}", 
                            target_dir, e));
            } else {
                add_message(&listview, "DEBUG",
                    &format!("Deleted existing directory at {:?}", 
                            target_dir));
            }
        }
        if let Err(e) = fs::remove_file(&shortcut_path) {
            add_message(&listview, "ERROR",
                &format!("Failed to delete shortcut '{:?}': {}", 
                    shortcut_path, e));
        } else {
            add_message(&listview, "DEBUG", &format!("Deleted shortcut at {:?}", 
                    shortcut_path));
        }
    } else {
        add_message(&listview, "DEBUG", &format!(
                "No existing shortcut found. Checking default location."));
        if let Some(local_appdata) = get_local_appdata(&listview) {
            let dir_to_delete = local_appdata.join(app_name);
            if dir_to_delete.exists() {
                if let Err(e) = fs::remove_dir_all(&dir_to_delete) {
                    add_message(&listview, "ERROR",
                        &format!("Failed to delete directory '{:?}': {}", 
                                dir_to_delete, e));
                } else {
                    add_message(&listview, "DEBUG",
                        &format!("Deleted existing directory at {:?}", 
                                dir_to_delete));
                }
            }
        }
    }
}

fn copy_latest_zip(listview: &nwg::ListView, bar: &nwg::ProgressBar, 
        app_name: &str) -> Option<PathBuf> {
    let source_dir_path = Path::new(r"C:\dev\apps").join(app_name);
    add_message(&listview, "DEBUG",
        &format!("Searching for zip files in {:?}", source_dir_path));

    let entries = match fs::read_dir(&source_dir_path) {
        Ok(entries) => entries,
        Err(e) => {
            add_message(&listview, "ERROR", &format!(
                    "Source directory not found or unreadable: {:?}: {}",
                    source_dir_path, e));
            return None;
        }
    };

    let mut newest_file: Option<(PathBuf, SystemTime)> = None;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file()
                && path.extension().and_then(|s| s.to_str()) == Some("zip")
            {
                if let Ok(metadata) = fs::metadata(&path) {
                    if let Ok(modified) = metadata.modified() {
                        if newest_file.is_none() ||
                            modified > newest_file.as_ref().unwrap().1 {
                            newest_file = Some((path, modified));
                        }
                    }
                }
            }
        }
    }

    if let Some((newest_file_path, _)) = newest_file.clone() {
        add_message(&listview, "DEBUG",
            &format!("Found latest zip file: {:?}", newest_file_path));
        if let Some(local_appdata) = get_local_appdata(&listview) {
            let file_name = match newest_file_path.file_name() {
                Some(name) => name,
                None => {
                    add_message(&listview, "ERROR",
                            "Could not get file name from path."); 
                    return None;
                }
            };
            let dest_path = local_appdata.join(file_name);

            //ui::show_progress();
            let result = copy_with_progress(&bar, 
                        &newest_file_path, &dest_path);
	    //ui::hide_progress();

            match result {
                Ok(_) => {
                    add_message(&listview, "DEBUG", &format!(
                            "Copied latest version {:?} to {:?}", 
                            file_name, dest_path)); 
                    return Some(dest_path);
                }
                Err(e) => {
                    add_message(&listview, "ERROR", 
                        &format!("Error copying file: {}", e));
                    return None;
                },
            }
        } else {
            add_message(&listview, "ERROR", 
                    "Could not find LOCALAPPDATA directory.");
        }
    } else {
        add_message(&listview, "ERROR", 
                &format!("No .zip files found in {:?}", source_dir_path)); 
    }
    None
}

fn update_progress(bar: &nwg::ProgressBar, progress: u32) {
    if progress < 100 {
        bar.set_pos(progress);
    } else {
        bar.set_pos(0);
    }
}

fn unzip_file(listview: &nwg::ListView, zip_file: &Path, app_name: &str) {
    if let Some(local_appdata) = get_local_appdata(&listview) {
        let extract_to_dir = local_appdata.join(app_name);
        if let Err(e) = fs::create_dir_all(&extract_to_dir) {
            add_message(&listview, "ERROR",
                &format!("Failed to create directory {:?}: {}", 
                        extract_to_dir, e));
            return;
        }

        let mut file = match File::open(zip_file) {
            Ok(f) => f,
            Err(e) => {
                add_message(&listview, "ERROR", 
                        &format!("Unable to open zip file: {}", e));
                return;
            }
        };

        let mut buffer = Vec::new();
        if let Err(e) = file.read_to_end(&mut buffer) {
            add_message(&listview, "ERROR", &format!(
                    "Unable to read zip file: {}", e));
            return;
        }

        let entries = match zip_utils::parse_central_directory(&buffer) {
            Ok(entries) => entries,
            Err(e) => {
                add_message(&listview, "ERROR", 
                        &format!("Failed to parse zip file: {}", e));
                return;
            }
        };

        for entry in &entries {
            add_message(&listview, "INFO", &format!("Extracting file: {}", 
                    entry.file_name));
            if let Err(e) = zip_utils::extract_file(entry, &buffer, 
                    &extract_to_dir) {
                add_message( &listview, "ERROR",
                    &format!("Failed to extract {}: {}", entry.file_name, e));
            }
        }

        add_message( &listview, "INFO", &format!(
                "Successfully unzipped to '{:?}'", extract_to_dir));
    } else {
        add_message(&listview, "ERROR", 
                "Could not find LOCALAPPDATA to unzip.");
    }
}

fn find_executable(dir: &Path) -> Option<PathBuf> {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file()
                    && path.extension().and_then(|s| s.to_str()) == Some("exe")
                {
                    return Some(path);
                }
            }
        }
    }
    None
}

fn create_shortcut(listview: &nwg::ListView, executable_path: &str, 
            shortcut_name: &str) {
    let start_menu_paths = get_start_menu_paths();
    if let Some(start_menu) = start_menu_paths
        .iter()
        .find(|p| p.to_str().unwrap_or("").contains("Local"))
        .or_else(|| start_menu_paths.first()) { 
        let shortcut_name_with_spaces = add_spaces(shortcut_name);
        let shortcut_path = start_menu.join(format!(
            "{}.lnk",
            shortcut_name_with_spaces
        ));
        if shortcut_path.exists() {
            if let Err(e) = fs::remove_file(&shortcut_path) {
                add_message(&listview, "ERROR",
                    &format!("Failed to delete existing shortcut: {}", e));
            }
        }

        let sl = match ShellLink::new(executable_path) {
            Ok(link) => link,
            Err(e) => {
                add_message(&listview, "ERROR",
                    &format!("Failed to create shell link: {}", e));
                return;
            }
        };

        if let Err(e) = sl.create_lnk(&shortcut_path) {
            add_message(&listview, "ERROR", &format!(
                    "Failed to create shortcut: {}", e));
        } else {
            add_message(&listview, "DEBUG", 
                    &format!("Shortcut created at {:?}", shortcut_path));
        }
    } else {
        add_message(&listview, "ERROR", "Could not find Start Menu path.");
    }
}

fn get_local_appdata_root() -> Option<PathBuf> {
    let mut path_ptr: PWSTR = std::ptr::null_mut();
    let result = unsafe { 
        SHGetKnownFolderPath(
            &FOLDERID_LocalAppData,
            0,
            std::ptr::null_mut(),
            &mut path_ptr
        ) 
    };
    if result == S_OK {
        let len = unsafe { 
            (0..).take_while(|&i| *path_ptr.offset(i) != 0).count() 
        };
        let path_slice = unsafe { 
            std::slice::from_raw_parts(path_ptr, len) 
        };
        let os_string: OsString = OsStringExt::from_wide(path_slice);
        Some(PathBuf::from(os_string))
    } else {
        None
    }
}

fn get_start_menu_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    let mut path_buf = [0u16; 300];
    unsafe {
        if SHGetSpecialFolderPathW(
            std::ptr::null_mut(),
            path_buf.as_mut_ptr(),
            CSIDL_STARTMENU,
            0
        ) != 0 {
            let path_str = String::from_utf16_lossy(&path_buf);
            let path_str = path_str.trim_end_matches('\0');
            paths.push(PathBuf::from(path_str));
        }
    }

    if let Some(mut local_appdata) = get_local_appdata_root() {
        local_appdata.push(r"Microsoft\Windows\Start Menu\Programs");
        if local_appdata.exists() {
            paths.push(local_appdata);
        }
    }

    paths
}

fn get_installer(listview: &nwg::ListView, bar: &nwg::ProgressBar) {
    if let Some(copied_zip_path) = copy_latest_zip(&listview, &bar, 
            "AppInstaller") {
        unzip_file(&listview, &copied_zip_path, "AppInstaller");
        if let Err(e) = fs::remove_file(&copied_zip_path) {
            add_message(&listview, "ERROR",
                &format!("Failed to delete installer zip file: {}", e));
        }
    } else {
        add_message(&listview, "ERROR", "Failed to download installer.");
    }
}

fn add_spaces(app_name: &str) -> String {
    let mut new_name = String::new();
    let mut last_char_was_lowercase = false;

    for c in app_name.chars() {
        if c.is_uppercase() && last_char_was_lowercase {
            new_name.push(' ');
        }
        new_name.push(c);
        last_char_was_lowercase = c.is_lowercase();
    }
    new_name
}

fn find_shortcut(shortcut_name: &str) -> Option<(PathBuf, PathBuf)> {
    for start_menu in get_start_menu_paths() {
        let shortcut_path = start_menu.join(format!("{}.lnk", shortcut_name));
        if shortcut_path.exists() {
            if let Ok(file) = File::open(&shortcut_path) {
                let mut reader = BufReader::new(file);
                if let Ok(link) = Lnk::new(&mut reader) {
                    if let Some(target) = link.link_info.local_base_path {
                        let target_path = PathBuf::from(target);
                        if let Some(parent) = target_path.parent() {
                            return Some((shortcut_path, parent.to_path_buf()));
                        }
                    }
                }
            }
        }
    }
    None
}

fn copy_with_progress(bar: &nwg::ProgressBar, from: &Path, to: &Path) -> 
        io::Result<()> {
    let mut from_file = File::open(from)?;
    let mut to_file = File::create(to)?;
    let file_size = from_file.metadata()?.len();
    let mut buffer = [0; 8192];
    let mut bytes_copied = 0;

    loop {
        let bytes_read = from_file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        to_file.write_all(&buffer[..bytes_read])?;
        bytes_copied += bytes_read as u64;
        let progress = (bytes_copied * 100 / file_size) as u32;
        update_progress(&bar, progress);
    }
    Ok(())
}

fn main() {
    nwg::init().expect("Failed to init Native Windows GUI");
    nwg::Font::set_global_family("Segoe UI").expect(
            "Failed to set default font");
    let args: Vec<String> = env::args().skip(1).collect();
    let mut app_name: String = "AppInstaller".to_string();
    let mut debug_mode = false;

    for arg in args {
        if arg == "--debug" {
            debug_mode = true;
        } else {
            app_name = arg;
        }
    }

    if app_name == "AppInstaller" {
        eprintln!("Error: No application name argument provided.");
        std::process::exit(1);
    }

    *APP_NAME.lock().unwrap() = Some(app_name.clone().into_boxed_str());
    *DEBUG.lock().unwrap() = debug_mode;
    let ui = FlexBoxApp::build_ui(Default::default()).expect(
            "Failed to build UI");
    run_installation(&ui.listview, &ui.progress_bar, &app_name);
    nwg::dispatch_thread_events();
}

