// W wydaniu na Windows bez okna konsoli.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    luzik_lib::run()
}
