extern crate native_windows_gui as nwg;
use nwg::NativeUi;
use std::env;
use std::path::PathBuf;
use once_cell::sync::Lazy;
use std::sync::Mutex;

mod zip_utils;
mod install_utils;
use install_utils::*;

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

