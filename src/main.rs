use fltk::{
    app,
    button::Button,
    browser::MultiBrowser,
    misc::Progress,
    prelude::*,
    window::Window,
    text::{TextBuffer, StyleTableEntry, TextDisplay},
};
use std::thread;
use std::path::{Path, PathBuf};
use std::fs;
use std::io::{self, Read, Write};
use std::time::SystemTime;
use chrono::Local;
use sysinfo::{System, SystemExt};
use std::process::Command;

mod zip_utils;

// Enum for messages sent from the worker thread to the UI thread
pub enum Message {
    Log(String, String),
    Progress(f32),
    InstallationComplete(Result<String, String>),
}

fn main() {
    let app = app::App::default().with_scheme(app::Scheme::Gtk);
    let mut wind = Window::new(100, 100, 600, 400, "App Installer");

    let mut browser = MultiBrowser::new(10, 25, 580, 315, "");
    browser.set_column_widths(&[80, 100, 400]);
    browser.set_column_char('\t');
    browser.add("Type\tTime\tMessage"); // Set headers

    let mut progress = Progress::new(10, 345, 580, 20, "");
    progress.set_minimum(0.0);
    progress.set_maximum(100.0);

    let mut copy_button = Button::new(190, 370, 100, 25, "Copy Log");
    let mut close_button = Button::new(310, 370, 100, 25, "Close");
    
    copy_button.deactivate();
    close_button.deactivate();

    wind.end();
    wind.show();

    let (sender, receiver) = app::channel::<Message>();

    // Run installation in a separate thread right away
    thread::spawn(move || {
        let app_name = "AppInstaller".to_string(); // Or parse from args if needed
        run_installation(sender, app_name);
    });

    while app.wait() {
        if let Some(msg) = receiver.recv() {
            match msg {
                Message::Log(msg_type, text) => {
                    let time_str = Local::now().format("%H:%M:%S").to_string();
                    let log_line = format!("{}\t{}\t{}", msg_type, time_str, text);
                    browser.add(&log_line);
                    browser.bottom_line(browser.size());
                }
                Message::Progress(val) => {
                    progress.set_value(val as f64);
                }
                Message::InstallationComplete(result) => {
                    match result {
                        Ok(exe_path) => {
                            add_log_entry(&mut browser, "INFO", "Installation successful!");
                            let _ = Command::new(exe_path).spawn(); // Attempt to run the app
                        }
                        Err(e) => {
                            add_log_entry(&mut browser, "ERROR", &format!("Installation failed: {}", e));
                        }
                    }
                    copy_button.activate();
                    close_button.activate();
                }
            }
        }
    }
}

fn add_log_entry(browser: &mut MultiBrowser, msg_type: &str, text: &str) {
    let time_str = Local::now().format("%H:%M:%S").to_string();
    let log_line = format!("{}\t{}\t{}", msg_type, time_str, text);
    browser.add(&log_line);
    browser.bottom_line(browser.size());
}

// --- Installation Logic (adapted for message passing) ---

fn run_installation(sender: app::Sender<Message>, app_name: String) {
    sender.send(Message::Log("INFO".into(), format!("Starting installation for {}", app_name)));

    let process_name = format!("{}.exe", app_name);
    if check_if_running(&process_name) {
        sender.send(Message::InstallationComplete(Err(format!("Application '{}' is running. Please close it and try again.", app_name))));
        return;
    }

    if let Err(e) = delete_directory(&sender, &app_name) {
        sender.send(Message::InstallationComplete(Err(format!("Failed to delete old directory: {}", e))));
        return;
    }

    match copy_latest_zip(&sender, &app_name) {
        Some(zip_path) => {
            if let Err(e) = unzip_file(&sender, &zip_path, &app_name) {
                sender.send(Message::InstallationComplete(Err(format!("Failed to unzip file: {}", e))));
                return;
            }

            let exe_path = find_executable(&app_name).unwrap_or_default();
            if exe_path.to_str().unwrap_or("").is_empty() {
                 sender.send(Message::InstallationComplete(Err("Could not find executable after extraction.".into())));
                 return;
            }
            
            if let Err(e) = fs::remove_file(&zip_path) {
                 sender.send(Message::Log("WARN".into(), format!("Failed to delete temporary zip file: {}", e)));
            }

            sender.send(Message::InstallationComplete(Ok(exe_path.to_string_lossy().to_string())));
        }
        None => {
            sender.send(Message::InstallationComplete(Err("Could not find or copy the latest zip file.".into())));
        }
    }
}

fn check_if_running(process_name: &str) -> bool {
    let s = System::new_all();
    let mut processes = s.processes_by_name(process_name);
    processes.next().is_some()
}

fn delete_directory(sender: &app::Sender<Message>, app_name: &str) -> io::Result<()> {
    if let Some(local_appdata) = dirs::data_local_dir() {
        let dir_to_delete = local_appdata.join("Utils").join(app_name);
        if dir_to_delete.exists() {
            sender.send(Message::Log("INFO".into(), format!("Deleting existing directory at {:?}", dir_to_delete)));
            fs::remove_dir_all(&dir_to_delete)?;
        }
    }
    Ok(())
}

fn copy_latest_zip(sender: &app::Sender<Message>, app_name: &str) -> Option<PathBuf> {
    let source_dir_path = Path::new(r"C:\dev\apps").join(app_name);
    sender.send(Message::Log("INFO".into(), format!("Searching for zip files in {:?}", source_dir_path)));

    let entries = fs::read_dir(&source_dir_path).ok()?;
    
    let newest_file = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file() && e.path().extension().and_then(|s| s.to_str()) == Some("zip"))
        .max_by_key(|e| e.metadata().ok().map(|m| m.modified().unwrap_or(SystemTime::UNIX_EPOCH)).unwrap_or(SystemTime::UNIX_EPOCH));

    if let Some(newest_entry) = newest_file {
        let newest_file_path = newest_entry.path();
        if let Some(local_appdata) = dirs::data_local_dir() {
            let dest_dir = local_appdata.join("Utils");
            fs::create_dir_all(&dest_dir).ok()?;
            let dest_path = dest_dir.join(newest_file_path.file_name().unwrap());
            
            sender.send(Message::Log("INFO".into(), format!("Copying {:?} to {:?}", newest_file_path, dest_path)));
            
            // Simplified copy without progress for now
            fs::copy(&newest_file_path, &dest_path).ok()?;
            sender.send(Message::Progress(100.0));

            return Some(dest_path);
        }
    }
    None
}

fn unzip_file(sender: &app::Sender<Message>, zip_file: &Path, app_name: &str) -> io::Result<()> {
    if let Some(local_appdata) = dirs::data_local_dir() {
        let extract_to_dir = local_appdata.join("Utils").join(app_name);
        fs::create_dir_all(&extract_to_dir)?;

        let file_content = fs::read(zip_file)?;
        let archive = zip_utils::parse_central_directory(&file_content)?;

        for (i, entry) in archive.iter().enumerate() {
            let progress = ((i + 1) as f32 / archive.len() as f32) * 100.0;
            sender.send(Message::Progress(progress));
            sender.send(Message::Log("INFO".into(), format!("Extracting: {}", entry.file_name)));
            zip_utils::extract_file(entry, &file_content, &extract_to_dir)?;
        }
        return Ok(())
    }
    Err(io::Error::new(io::ErrorKind::NotFound, "Could not find local app data directory"))
}

fn find_executable(app_name: &str) -> Option<PathBuf> {
    if let Some(local_appdata) = dirs::data_local_dir() {
        let app_dir = local_appdata.join("Utils").join(app_name);
        if let Ok(entries) = fs::read_dir(app_dir) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("exe") {
                    return Some(path);
                }
            }
        }
    }
    None
}
