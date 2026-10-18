#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    let root = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_default();
    let plugins = if root.join("plugins").is_dir() {
        root.join("plugins")
    } else {
        std::env::current_dir().unwrap_or_default().join("plugins")
    };
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let path = args
        .first()
        .filter(|s| !s.to_string_lossy().starts_with('-'))
        .map(std::path::PathBuf::from);
    let document = if args.iter().any(|a| a == "--demo") {
        cad::model::demo()
    } else if let Some(p) = &path {
        match cad::model::Document::load(p) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Cannot open {}: {e}", p.display());
                return Err(eframe::Error::AppCreation(e.into()));
            }
        }
    } else {
        cad::model::Document::default()
    };
    let space_mouse = cad::spacemouse::SpaceMouse::default();
    #[cfg(windows)]
    let input_hook = space_mouse.clone();
    let options = eframe::NativeOptions {
        #[cfg(windows)]
        event_loop_builder: Some(Box::new(move |builder| input_hook.install_hook(builder))),
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("CAD · Part Studio")
            .with_inner_size([1380., 900.])
            .with_min_inner_size([1024., 680.]),
        renderer: eframe::Renderer::Glow,
        depth_buffer: 24,
        multisampling: 4,
        vsync: true,
        ..Default::default()
    };
    eframe::run_native(
        "CAD",
        options,
        Box::new(move |cc| {
            Ok(Box::new(cad::app::CadApp::new(
                cc,
                document,
                path,
                plugins,
                space_mouse,
            )))
        }),
    )
}
