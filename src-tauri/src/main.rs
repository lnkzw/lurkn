// Windows release builds must not pop a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    lurkn_lib::run();
}
