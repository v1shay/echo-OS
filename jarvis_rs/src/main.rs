use std::sync::Arc;

use anyhow::Result;
use eframe::egui::ViewportBuilder;

use jarvis_rs::agent;
use jarvis_rs::app::JarvisApp;
use jarvis_rs::automation::LocalAutomationBackend;
use jarvis_rs::config::AppConfig;
use jarvis_rs::logging;

fn main() -> Result<()> {
    logging::init();

    let config = AppConfig::from_env();
    let runtime = Arc::new(tokio::runtime::Runtime::new()?);
    let automation = Arc::new(LocalAutomationBackend::new(config.allowed_paths.clone()));
    let started_agent = agent::start(runtime.handle(), config.clone(), automation);

    let native_options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Jarvis MVP")
            .with_inner_size([1080.0, 760.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Jarvis MVP",
        native_options,
        Box::new(move |_cc| {
            Ok(Box::new(JarvisApp::new(
                Arc::clone(&runtime),
                config,
                started_agent.commands,
                started_agent.events,
            )))
        }),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}
