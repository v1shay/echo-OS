use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    jarvis_rs::logging::init();

    // TODO: wire intent parsing -> safety validation -> LLM -> executor.

    Ok(())
}
