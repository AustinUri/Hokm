// crates/hokm_client/src/bin/gui.rs
//
// GUI entry point.

mod gui_app;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Hokm (GUI)",
        options,
        Box::new(|cc| Ok(Box::new(gui_app::GuiApp::new(cc)))),
    )
}
