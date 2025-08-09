#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::System::LibraryLoader::*,
    Win32::UI::Controls::*,
    Win32::UI::WindowsAndMessaging::*,
};

use chrono::Local;
use mslnk::ShellLink;
use std::env;
use std::ffi::CString;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr::null_mut;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime};
use sysinfo::{System, SystemExt};
use winapi::shared::winerror::S_OK;
use winapi::um::knownfolders::FOLDERID_LocalAppData;
use winapi::um::shlobj::SHGetKnownFolderPath;
use winapi::um::shlobj::SHGetSpecialFolderPathW;
use winapi::um::shlobj::CSIDL_STARTMENU;
use winapi::um::winbase::GlobalAlloc;
use winapi::um::winbase::GlobalLock;
use winapi::um::winbase::GlobalUnlock;
use winapi::um::winbase::GMEM_MOVEABLE;
use winapi::um::winnt::PWSTR;
use winapi::um::winuser::CloseClipboard;
use winapi::um::winuser::EmptyClipboard;
use winapi::um::winuser::OpenClipboard;
use winapi::um::winuser::SetClipboardData;
use winapi::um::winuser::CF_TEXT;

mod zip_utils;

const IDC_LISTVIEW: u16 = 1001;
const IDC_OK: u16 = 1002;
const IDC_CANCEL: u16 = 1003;
const IDC_PROGRESS: u16 = 1004;
const IDC_COPY_BUTTON: u16 = 1005;

struct AppState {
    list_hwnd: Option<HWND>,
    ok_hwnd: Option<HWND>,
    cancel_hwnd: Option<HWND>,
    progress_hwnd: Option<HWND>,
    copy_hwnd: Option<HWND>,
    app_name: String,
    debug: bool,
}

fn to_pstr_null(s: &str) -> PSTR {
    PSTR(CString::new(s).unwrap().into_raw() as *mut u8)
}

fn loword(l: u32) -> u16 {
    (l & 0xFFFF) as u16
}

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
}

fn get_local_appdata(app_state_arc: &Arc<Mutex<AppState>>) -> Option<PathBuf> {
    let mut path_ptr: PWSTR = std::ptr::null_mut();
    let result = unsafe {
        SHGetKnownFolderPath(
            &FOLDERID_LocalAppData,
            0,
            std::ptr::null_mut(),
            &mut path_ptr,
        )
    };
    if result == S_OK {
        let len = unsafe { (0..).take_while(|&i| *path_ptr.offset(i) != 0).count() };
        let path_slice = unsafe { std::slice::from_raw_parts(path_ptr, len) };
        let os_string: OsString = OsStringExt::from_wide(path_slice);
        let mut path = PathBuf::from(os_string);
        path.push("Utils");
        if !path.exists() {
            if let Err(e) = fs::create_dir_all(&path) {
                add_message(
                    app_state_arc,
                    "ERROR",
                    &format!("Failed to create directory {:?}: {}", path, e),
                );
                return None;
            }
        }
        Some(path)
    } else {
        None
    }
}

fn add_message(app_state_arc: &Arc<Mutex<AppState>>, message_type: &str, message: &str) {
    let app_state = app_state_arc.lock().unwrap();
    if message_type == "DEBUG" && !app_state.debug {
        return;
    }
    if let Some(list_hwnd) = app_state.list_hwnd {
        let time_str = Local::now().format("%H:%M:%S").to_string();
        let index = unsafe { SendMessageA(list_hwnd, LVM_GETITEMCOUNT, WPARAM(0), LPARAM(0)) }.0;

        let item = LVITEMA {
            mask: LVIF_TEXT,
            iItem: index as i32,
            iSubItem: 0,
            pszText: to_pstr_null(message_type),
            ..Default::default()
        };
        unsafe {
            SendMessageA(
                list_hwnd,
                LVM_INSERTITEMA,
                WPARAM(0),
                LPARAM(&item as *const _ as _),
            );
        }

        let time_item = LVITEMA {
            mask: LVIF_TEXT,
            iItem: index as i32,
            iSubItem: 1,
            pszText: to_pstr_null(&time_str),
            ..Default::default()
        };
        unsafe {
            SendMessageA(
                list_hwnd,
                LVM_SETITEMA,
                WPARAM(0),
                LPARAM(&time_item as *const _ as _),
            );
        }

        let msg_item = LVITEMA {
            mask: LVIF_TEXT,
            iItem: index as i32,
            iSubItem: 2,
            pszText: to_pstr_null(message),
            ..Default::default()
        };
        unsafe {
            SendMessageA(
                list_hwnd,
                LVM_SETITEMA,
                WPARAM(0),
                LPARAM(&msg_item as *const _ as _),
            );
        }
    }
}

fn main() {
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

    let app_state = Arc::new(Mutex::new(AppState {
        list_hwnd: None,
        ok_hwnd: None,
        cancel_hwnd: None,
        progress_hwnd: None,
        copy_hwnd: None,
        app_name,
        debug: debug_mode,
    }));

    if let Err(e) = create_window(app_state) {
        eprintln!("Error creating window: {:?}", e);
    }
}

fn create_window(app_state: Arc<Mutex<AppState>>) -> Result<()> {
    let app_state_box = Box::new(app_state);
    let app_state_ptr = Box::into_raw(app_state_box);

    unsafe {
        let h_instance = GetModuleHandleA(None)?;
        let class_name = s!("AppInstaller");
        let wc = WNDCLASSA {
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hInstance: h_instance.into(),
            lpszClassName: class_name,
            lpfnWndProc: Some(wnd_proc),
            style: CS_HREDRAW | CS_VREDRAW,
            hbrBackground: HBRUSH(COLOR_WINDOW.0 as isize),
            ..Default::default()
        };

        RegisterClassA(&wc);

        let _hwnd = CreateWindowExA(
            WINDOW_EX_STYLE(0),
            class_name,
            s!("AppInstaller"),
            WINDOW_STYLE(WS_OVERLAPPEDWINDOW.0 | WS_VISIBLE.0),
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            600,
            400,
            None,
            None,
            h_instance,
            Some(app_state_ptr as *mut _),
        );

        let mut msg = MSG::default();
        while GetMessageA(&mut msg, HWND(0), 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageA(&msg);
        }
    }
    Ok(())
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let app_state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut Arc<Mutex<AppState>>;

    match msg {
        WM_NCCREATE => {
            let createstruct = unsafe { &*(lparam.0 as *const CREATESTRUCTA) };
            let app_state_ptr = createstruct.lpCreateParams as *mut Arc<Mutex<AppState>>;
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, app_state_ptr as isize) };
            return unsafe { DefWindowProcA(hwnd, msg, wparam, lparam) };
        }
        WM_CREATE => {
            if !app_state_ptr.is_null() {
                let app_state_arc = unsafe { &*app_state_ptr };
                return handle_wm_create(hwnd, app_state_arc);
            }
            return LRESULT(1);
        }
        WM_DESTROY => {
            if !app_state_ptr.is_null() {
                let _ = unsafe { Box::from_raw(app_state_ptr) };
            }
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        _ => {
            if !app_state_ptr.is_null() {
                let app_state_arc = unsafe { &*app_state_ptr };
                match msg {
                    WM_NOTIFY => return handle_wm_notify(lparam, app_state_arc),
                    WM_SIZE => handle_wm_size(lparam, app_state_arc),
                    WM_COMMAND => handle_wm_command(wparam, app_state_arc),
                    _ => {}
                }
            }
        }
    }
    unsafe { DefWindowProcA(hwnd, msg, wparam, lparam) }
}

fn handle_wm_create(hwnd: HWND, app_state_arc: &Arc<Mutex<AppState>>) -> LRESULT {
    let h_instance = unsafe { GetModuleHandleA(None).unwrap() };
    let mut app_state = app_state_arc.lock().unwrap();

    let list_hwnd = unsafe { CreateWindowExA(
        WINDOW_EX_STYLE(0),
        s!("SysListView32"),
        None,
        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | LVS_REPORT | WS_BORDER.0),
        10, 10, 560, 280,
        hwnd, HMENU(IDC_LISTVIEW as isize), h_instance, Some(null_mut()),
    ) };
    app_state.list_hwnd = Some(list_hwnd);

    let progress_hwnd = unsafe { CreateWindowExA(
        WINDOW_EX_STYLE(0),
        PROGRESS_CLASSA,
        None,
        WINDOW_STYLE(WS_CHILD.0 | PBS_SMOOTH),
        10, 295, 560, 20,
        hwnd, HMENU(IDC_PROGRESS as isize), h_instance, Some(null_mut()),
    ) };
    app_state.progress_hwnd = Some(progress_hwnd);

    let list_style = unsafe { SendMessageA(list_hwnd, LVM_GETEXTENDEDLISTVIEWSTYLE, WPARAM(0), LPARAM(0)) };
    unsafe { SendMessageA(list_hwnd, LVM_SETEXTENDEDLISTVIEWSTYLE, WPARAM(0), LPARAM((list_style.0 as u32 | LVS_EX_FULLROWSELECT) as isize)) };

    let columns = ["Type", "Time", "Message"];
    for (i, &col_name) in columns.iter().enumerate() {
        let lvc = LVCOLUMNA {
            mask: LVCF_TEXT | LVCF_WIDTH | LVCF_SUBITEM,
            cx: if i == 2 { 300 } else { 100 },
            pszText: to_pstr_null(col_name),
            iSubItem: i as i32,
            ..Default::default()
        };
        unsafe { SendMessageA(list_hwnd, LVM_INSERTCOLUMNA, WPARAM(i as usize), LPARAM(&lvc as *const _ as _)) };
    }
    
    drop(app_state);

    add_message(app_state_arc, "INFO", "System initialized.");
    
    let app_state_clone = Arc::clone(app_state_arc);
    thread::spawn(move || {
        run_installation(&app_state_clone);
    });

    let mut app_state = app_state_arc.lock().unwrap();

    let btn_width = 120;
    let btn_height = 30;
    let spacing = 20;
    let total_width = btn_width * 3 + spacing * 2;
    let x_start = (600 - total_width) / 2;

    let ok_hwnd = unsafe { CreateWindowExA(
        WINDOW_EX_STYLE(0),
        s!("BUTTON"), s!("Close"),
        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | BS_DEFPUSHBUTTON as u32),
        x_start, 325, btn_width, btn_height,
        hwnd, HMENU(IDC_OK as isize), h_instance, Some(null_mut()),
    ) };
    app_state.ok_hwnd = Some(ok_hwnd);

    let copy_hwnd = unsafe { CreateWindowExA(
        WINDOW_EX_STYLE(0),
        s!("BUTTON"), s!("Copy to Clipboard"),
        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0),
        x_start + btn_width + spacing, 325, btn_width, btn_height,
        hwnd, HMENU(IDC_COPY_BUTTON as isize), h_instance, Some(null_mut()),
    ) };
    app_state.copy_hwnd = Some(copy_hwnd);

    let cancel_hwnd = unsafe { CreateWindowExA(
        WINDOW_EX_STYLE(0),
        s!("BUTTON"), s!("Cancel"),
        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0),
        x_start + (btn_width + spacing) * 2, 325, btn_width, btn_height,
        hwnd, HMENU(IDC_CANCEL as isize), h_instance, Some(null_mut()),
    ) };
    app_state.cancel_hwnd = Some(cancel_hwnd);
    
    LRESULT(0)
}

fn handle_wm_notify(lparam: LPARAM, app_state_arc: &Arc<Mutex<AppState>>) -> LRESULT {
    let app_state = app_state_arc.lock().unwrap();
    let nmhdr = unsafe { &*(lparam.0 as *const NMHDR) };
    if nmhdr.idFrom == IDC_LISTVIEW as usize && nmhdr.code == NM_CUSTOMDRAW {
        let cd = unsafe { &mut *(lparam.0 as *mut NMLVCUSTOMDRAW) };
        match cd.nmcd.dwDrawStage {
            CDDS_PREPAINT => return LRESULT(CDRF_NOTIFYITEMDRAW as isize),
            CDDS_ITEMPREPAINT => {
                let mut text_buf = [0u8; 64];
                let lvi = LVITEMA {
                    mask: LVIF_TEXT,
                    iItem: cd.nmcd.dwItemSpec as i32,
                    iSubItem: 0,
                    pszText: PSTR(text_buf.as_mut_ptr()),
                    cchTextMax: text_buf.len() as i32,
                    ..Default::default()
                };
                if let Some(list_hwnd) = app_state.list_hwnd {
                    unsafe { SendMessageA(list_hwnd, LVM_GETITEMA, WPARAM(0), LPARAM(&lvi as *const _ as _)) };
                }

                let msg_type = std::str::from_utf8(&text_buf)
                    .unwrap_or("")
                    .trim_end_matches(char::from(0));

                cd.clrText = match msg_type {
                    "ERROR" => rgb(255, 0, 0),
                    "DEBUG" => rgb(0, 0, 255),
                    _ => rgb(0, 0, 0),
                };
                return LRESULT(CDRF_NEWFONT as isize);
            }
            _ => {}
        }
    }
    LRESULT(0)
}

fn handle_wm_size(lparam: LPARAM, app_state_arc: &Arc<Mutex<AppState>>) {
    let app_state = app_state_arc.lock().unwrap();
    let width = loword(lparam.0 as u32) as i32;
    let height = (lparam.0 >> 16) as i32;

    let margin = 10;
    let button_height = 30;
    let button_width = 120;
    let spacing = 20;
    let total_button_width = button_width * 3 + spacing * 2;
    let button_y = height - button_height - margin;
    let button_x = (width - total_button_width) / 2;

    if let Some(list_hwnd) = app_state.list_hwnd {
        unsafe { _ = MoveWindow(list_hwnd, margin, margin, width - margin * 2, button_y - margin * 2 - 25, true); };
        let total_width = width - margin * 2;
        let col0_width = 100;
        let col1_width = 100;
        let col2_width = total_width - col0_width - col1_width;
        let col = LVCOLUMNW { mask: LVCF_WIDTH, cx: col2_width, ..Default::default() };
        unsafe { SendMessageA(list_hwnd, LVM_SETCOLUMNW, WPARAM(2), LPARAM(&col as *const _ as _)) };
    }
    if let Some(progress_hwnd) = app_state.progress_hwnd {
        unsafe { _ = MoveWindow(progress_hwnd, margin, button_y - 25, width - margin * 2, 20, true) };
    }
    if let Some(ok_hwnd) = app_state.ok_hwnd {
        unsafe { _ = MoveWindow(ok_hwnd, button_x, button_y, button_width, button_height, true) };
    }
    if let Some(copy_hwnd) = app_state.copy_hwnd {
        unsafe { _ = MoveWindow(copy_hwnd, button_x + button_width + spacing, button_y, button_width, button_height, true) };
    }
    if let Some(cancel_hwnd) = app_state.cancel_hwnd {
        unsafe { _ = MoveWindow(cancel_hwnd, button_x + (button_width + spacing) * 2, button_y, button_width, button_height, true) };
    }
}

fn handle_wm_command(wparam: WPARAM, app_state_arc: &Arc<Mutex<AppState>>) {
    match loword(wparam.0 as u32) {
        x if x == IDC_OK || x == IDC_CANCEL => unsafe { PostQuitMessage(0) },
        x if x == IDC_COPY_BUTTON => copy_to_clipboard(app_state_arc),
        _ => {}
    }
}

fn run_installation(app_state_arc: &Arc<Mutex<AppState>>) {
    update_installer(app_state_arc);
    let app_name = app_state_arc.lock().unwrap().app_name.clone();

    add_message(app_state_arc, "INFO", &format!("Starting installation for {}", app_name));

    let process_name = format!("{}.exe", app_name);
    if check_if_running(&process_name) {
        add_message(
            app_state_arc,
            "ERROR",
            &format!("Application '{}' is running. Please close it and try again.", app_name),
        );
        return;
    }

    delete_directory(app_state_arc, &app_name);

    if let Some(copied_zip_path) = copy_latest_zip(app_state_arc, &app_name) {
        unzip_file(app_state_arc, &copied_zip_path, &app_name);

        if let Some(local_appdata) = get_local_appdata(app_state_arc) {
            let app_dir = local_appdata.join(&app_name);
            if let Some(exe_path) = find_executable(&app_dir) {
                add_message(app_state_arc, "DEBUG", &format!("Found executable at {:?}", exe_path));
                if let Some(exe_str) = exe_path.to_str() {
                    create_shortcut(app_state_arc, exe_str, &app_name);
                    if let Err(e) = Command::new(&exe_path).spawn() {
                        add_message(app_state_arc, "ERROR", &format!("Failed to start application: {}", e));
                    } else {
                        add_message(app_state_arc, "INFO", &format!("Successfully started {}", app_name));
                    }
                } else {
                    add_message(app_state_arc, "ERROR", "Executable path contains invalid characters.");
                }
            } else {
                add_message(app_state_arc, "ERROR", &format!("Could not find executable for {}", app_name));
            }
        }

        if let Err(e) = fs::remove_file(&copied_zip_path) {
            add_message(app_state_arc, "ERROR", &format!("Failed to delete temporary zip file: {}", e));
        }
    } else {
        add_message(app_state_arc, "ERROR", &format!("Installation failed for {}.", app_name));
    }
    add_message(app_state_arc, "INFO", "Installation process finished.");
}

fn find_executable(dir: &Path) -> Option<PathBuf> {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("exe") {
                    return Some(path);
                }
            }
        }
    }
    None
}

fn copy_latest_zip(app_state_arc: &Arc<Mutex<AppState>>, app_name: &str) -> Option<PathBuf> {
    let source_dir_path = Path::new(r"C:\dev\apps").join(app_name);
    add_message(app_state_arc, "DEBUG", &format!("Searching for zip files in {:?}", source_dir_path));

    let entries = match fs::read_dir(&source_dir_path) {
        Ok(entries) => entries,
        Err(e) => {
            add_message(app_state_arc, "ERROR", &format!("Source directory not found or unreadable: {:?}: {}", source_dir_path, e));
            return None;
        }
    };

    let mut newest_file: Option<(PathBuf, SystemTime)> = None;
    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("zip") {
                if let Ok(metadata) = fs::metadata(&path) {
                    if let Ok(modified) = metadata.modified() {
                        if newest_file.is_none() || modified > newest_file.as_ref().unwrap().1 {
                            newest_file = Some((path, modified));
                        }
                    }
                }
            }
        }
    }

    if let Some((newest_file_path, _)) = newest_file {
        add_message(app_state_arc, "DEBUG", &format!("Found latest zip file: {:?}", newest_file_path));
        if let Some(local_appdata) = get_local_appdata(app_state_arc) {
            let file_name = newest_file_path.file_name().unwrap();
            let dest_path = local_appdata.join(file_name);
            
            let result = copy_with_progress(app_state_arc, &newest_file_path, &dest_path);

            match result {
                Ok(_) => {
                    add_message(app_state_arc, "INFO", &format!("Copied latest version {:?} to {:?}", file_name, dest_path));
                    Some(dest_path)
                }
                Err(e) => {
                    add_message(app_state_arc, "ERROR", &format!("Error copying file: {}", e));
                    None
                }
            }
        } else {
            add_message(app_state_arc, "ERROR", "Could not find LOCALAPPDATA directory.");
            None
        }
    } else {
        add_message(app_state_arc, "ERROR", &format!("No .zip files found in {:?}", source_dir_path));
        None
    }
}

fn copy_with_progress(app_state_arc: &Arc<Mutex<AppState>>, from: &Path, to: &Path) -> io::Result<()> {
    let mut from_file = File::open(from)?;
    let mut to_file = File::create(to)?;
    let file_size = from_file.metadata()?.len();
    let mut buffer = [0; 8192];
    let mut bytes_copied = 0;

    let app_state = app_state_arc.lock().unwrap();
    if let Some(progress_hwnd) = app_state.progress_hwnd {
        unsafe { _ = ShowWindow(progress_hwnd, SW_SHOW) };
    }
    drop(app_state);

    loop {
        let bytes_read = from_file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        to_file.write_all(&buffer[..bytes_read])?;
        bytes_copied += bytes_read as u64;
        let progress = (bytes_copied * 100 / file_size) as i32;
        
        let app_state = app_state_arc.lock().unwrap();
        if let Some(progress_hwnd) = app_state.progress_hwnd {
            unsafe { SendMessageA(progress_hwnd, PBM_SETPOS, WPARAM(progress as usize), LPARAM(0)) };
        }
    }
    
    let app_state = app_state_arc.lock().unwrap();
    if let Some(progress_hwnd) = app_state.progress_hwnd {
        unsafe { _ = ShowWindow(progress_hwnd, SW_HIDE) };
    }
    Ok(())
}

fn check_if_running(process_name: &str) -> bool {
    let s = System::new_all();
    for _process in s.processes_by_name(process_name) {
        return true;
    }
    false
}

fn delete_directory(app_state_arc: &Arc<Mutex<AppState>>, app_name: &str) {
    add_message(app_state_arc, "DEBUG", &format!("Attempting to delete directory for {}", app_name));
    if let Some(local_appdata) = get_local_appdata(app_state_arc) {
        let dir_to_delete = local_appdata.join(app_name);
        if !dir_to_delete.exists() {
            return;
        }
        if let Err(e) = fs::remove_dir_all(&dir_to_delete) {
            add_message(app_state_arc, "ERROR", &format!("Failed to delete directory '{:?}': {}", dir_to_delete, e));
        } else {
            add_message(app_state_arc, "INFO", &format!("Deleted existing directory at {:?}", dir_to_delete));
        }
    } else {
        add_message(app_state_arc, "ERROR", "Could not find LOCALAPPDATA path for deletion.");
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

fn create_shortcut(app_state_arc: &Arc<Mutex<AppState>>, executable_path: &str, shortcut_name: &str) {
    if let Some(start_menu) = get_start_menu_path() {
        let shortcut_name_with_spaces = add_spaces(shortcut_name);
        let shortcut_path = start_menu.join(format!("{}.lnk", shortcut_name_with_spaces));
        if shortcut_path.exists() {
            if let Err(e) = fs::remove_file(&shortcut_path) {
                add_message(app_state_arc, "ERROR", &format!("Failed to delete existing shortcut: {}", e));
            }
        }

        let sl = match ShellLink::new(executable_path) {
            Ok(link) => link,
            Err(e) => {
                add_message(app_state_arc, "ERROR", &format!("Failed to create shell link: {}", e));
                return;
            }
        };

        if let Err(e) = sl.create_lnk(&shortcut_path) {
            add_message(app_state_arc, "ERROR", &format!("Failed to create shortcut: {}", e));
        } else {
            add_message(app_state_arc, "INFO", &format!("Shortcut created at {:?}", shortcut_path));
        }
    } else {
        add_message(app_state_arc, "ERROR", "Could not find Start Menu path.");
    }
}

fn get_start_menu_path() -> Option<PathBuf> {
    let mut path_buf = [0u16; 300];
    unsafe {
        if SHGetSpecialFolderPathW(
            std::ptr::null_mut(),
            path_buf.as_mut_ptr(),
            CSIDL_STARTMENU,
            0,
        ) == 0
        {
            return None;
        }
    }
    let path_str = String::from_utf16_lossy(&path_buf);
    let path_str = path_str.trim_end_matches('\0');
    Some(PathBuf::from(path_str))
}

fn unzip_file(app_state_arc: &Arc<Mutex<AppState>>, zip_file: &Path, app_name: &str) {
    if let Some(local_appdata) = get_local_appdata(app_state_arc) {
        let extract_to_dir = local_appdata.join(app_name);
        if let Err(e) = fs::create_dir_all(&extract_to_dir) {
            add_message(app_state_arc, "ERROR", &format!("Failed to create directory {:?}: {}", extract_to_dir, e));
            return;
        }

        let mut file = match File::open(zip_file) {
            Ok(f) => f,
            Err(e) => {
                add_message(app_state_arc, "ERROR", &format!("Unable to open zip file: {}", e));
                return;
            }
        };

        let mut buffer = Vec::new();
        if let Err(e) = file.read_to_end(&mut buffer) {
            add_message(app_state_arc, "ERROR", &format!("Unable to read zip file: {}", e));
            return;
        }

        let entries = match zip_utils::parse_central_directory(&buffer) {
            Ok(entries) => entries,
            Err(e) => {
                add_message(app_state_arc, "ERROR", &format!("Failed to parse zip file: {}", e));
                return;
            }
        };

        for entry in &entries {
            add_message(app_state_arc, "INFO", &format!("Extracting file: {}", entry.file_name));
            if let Err(e) = zip_utils::extract_file(entry, &buffer, &extract_to_dir) {
                add_message(app_state_arc, "ERROR", &format!("Failed to extract {}: {}", entry.file_name, e));
            }
        }

        add_message(app_state_arc, "INFO", &format!("Successfully unzipped to '{:?}'", extract_to_dir));
    } else {
        add_message(app_state_arc, "ERROR", "Could not find LOCALAPPDATA to unzip.");
    }
}

fn copy_to_clipboard(app_state_arc: &Arc<Mutex<AppState>>) {
    let app_state = app_state_arc.lock().unwrap();
    if let Some(list_hwnd) = app_state.list_hwnd {
        let item_count = unsafe { SendMessageA(list_hwnd, LVM_GETITEMCOUNT, WPARAM(0), LPARAM(0)) }.0;
        let mut text_to_copy = String::new();

        for i in 0..item_count {
            let mut text_buf = [0u8; 256];
            let lvi = LVITEMA {
                mask: LVIF_TEXT,
                iItem: i as i32,
                iSubItem: 0,
                pszText: PSTR(text_buf.as_mut_ptr()),
                cchTextMax: text_buf.len() as i32,
                ..Default::default()
            };
            unsafe { SendMessageA(list_hwnd, LVM_GETITEMA, WPARAM(0), LPARAM(&lvi as *const _ as _)) };
            let time = String::from_utf8_lossy(&text_buf).trim_end_matches(char::from(0)).to_string();

            let lvi = LVITEMA {
                mask: LVIF_TEXT,
                iItem: i as i32,
                iSubItem: 1,
                pszText: PSTR(text_buf.as_mut_ptr()),
                cchTextMax: text_buf.len() as i32,
                ..Default::default()
            };
            unsafe { SendMessageA(list_hwnd, LVM_GETITEMA, WPARAM(0), LPARAM(&lvi as *const _ as _)) };
            let type_ = String::from_utf8_lossy(&text_buf).trim_end_matches(char::from(0)).to_string();

            let lvi = LVITEMA {
                mask: LVIF_TEXT,
                iItem: i as i32,
                iSubItem: 2,
                pszText: PSTR(text_buf.as_mut_ptr()),
                cchTextMax: text_buf.len() as i32,
                ..Default::default()
            };
            unsafe { SendMessageA(list_hwnd, LVM_GETITEMA, WPARAM(0), LPARAM(&lvi as *const _ as _)) };
            let message = String::from_utf8_lossy(&text_buf).trim_end_matches(char::from(0)).to_string();

            text_to_copy.push_str(&format!("{}\t{} \t{}\r\n", time, type_, message));
        }
        
        drop(app_state); 

        if unsafe { OpenClipboard(null_mut()) } != 0 {
            unsafe { EmptyClipboard() };

            let h_glob = unsafe { GlobalAlloc(GMEM_MOVEABLE, text_to_copy.len() + 1) };
            let p_glob = unsafe { GlobalLock(h_glob) };
            if !p_glob.is_null() {
                let p_glob_char = p_glob as *mut u8;
                unsafe { p_glob_char.copy_from(text_to_copy.as_ptr(), text_to_copy.len()) };
                unsafe { p_glob_char.add(text_to_copy.len()).write(0) };
                unsafe { GlobalUnlock(h_glob) };
                unsafe { SetClipboardData(CF_TEXT.into(), h_glob) };
            }
            unsafe { CloseClipboard() };
            add_message(app_state_arc, "INFO", "Log copied to clipboard.");
        } else {
            add_message(app_state_arc, "ERROR", "Failed to open clipboard.");
        }
    }
}

fn delete_old_installers(app_state_arc: &Arc<Mutex<AppState>>) {
    if let Ok(current_exe) = env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            if let Ok(entries) = fs::read_dir(parent) {
                for entry in entries {
                    if let Ok(entry) = entry {
                        let path = entry.path();
                        if let Some(extension) = path.extension() {
                            if extension == "old" {
                                if let Err(e) = fs::remove_file(&path) {
                                    add_message(app_state_arc, "ERROR", &format!("Failed to delete old installer: {}", e));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn update_installer(app_state_arc: &Arc<Mutex<AppState>>) {
    delete_old_installers(app_state_arc);
    add_message(app_state_arc, "INFO", "Checking for installer updates...");
    if let Some(local_appdata) = get_local_appdata(app_state_arc) {
        let local_dir = local_appdata.join("AppInstaller");
        if !local_dir.exists() {
            add_message(app_state_arc, "INFO", "No local installer found. Downloading...");
            get_installer(app_state_arc);
            return;
        }

        if let Ok(current_exe) = env::current_exe() {
            if let Ok(local_meta) = fs::metadata(&current_exe) {
                if let Ok(local_time) = local_meta.modified() {
                    let remote_dir = Path::new(r"C:\dev\apps").join("AppInstaller");
                    let mut newest_remote_file: Option<(PathBuf, SystemTime)> = None;
                    if let Ok(entries) = fs::read_dir(&remote_dir) {
                        for entry in entries {
                            if let Ok(entry) = entry {
                                let path = entry.path();
                                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("zip") {
                                    if let Ok(metadata) = fs::metadata(&path) {
                                        if let Ok(modified) = metadata.modified() {
                                            if newest_remote_file.is_none() || modified > newest_remote_file.as_ref().unwrap().1 {
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
                            add_message(app_state_arc, "INFO", "Newer installer found. Updating...");
                            let now = Local::now();
                            let timestamp = now.format("%Y%m%d%H%M%S");
                            let new_name = current_exe.with_extension(format!("exe.old_{}", timestamp));
                            if let Err(e) = fs::rename(&current_exe, &new_name) {
                                add_message(app_state_arc, "ERROR", &format!("Failed to rename old installer: {}", e));
                                return;
                            }
                            get_installer(app_state_arc);
                            add_message(app_state_arc, "INFO", "Installer updated. Please restart the application.");
                            unsafe { PostQuitMessage(0) };
                        } else {
                            add_message(app_state_arc, "INFO", "Installer is up to date.");
                        }
                    }
                }
            }
        }
    }
}

fn get_installer(app_state_arc: &Arc<Mutex<AppState>>) {
    if let Some(copied_zip_path) = copy_latest_zip(app_state_arc, "AppInstaller") {
        unzip_file(app_state_arc, &copied_zip_path, "AppInstaller");
        if let Err(e) = fs::remove_file(&copied_zip_path) {
            add_message(app_state_arc, "ERROR", &format!("Failed to delete temporary installer zip file: {}", e));
        }
    } else {
        add_message(app_state_arc, "ERROR", "Failed to download installer.");
    }
}