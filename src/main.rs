#![cfg_attr(not(test), windows_subsystem = "windows")]
mod ui;
mod viewer;
fn main() {
    if let Err(error) = ui::run() {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                std::ptr::null_mut(),
                ui::wide(&error).as_ptr(),
                ui::wide("gymL").as_ptr(),
                windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
            );
        }
    }
}
