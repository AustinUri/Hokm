// crates/hokm_client/src/main.rs
//
// This is the DEFAULT binary: the TUI.

mod tui;

fn main() -> std::io::Result<()> {
    tui::run()
}
