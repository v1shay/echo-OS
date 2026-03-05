mod intent;
mod executor;

use intent::*;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let fake_llm_output = r#"
    {
        "intent": "open_application",
        "parameters": {
            "app_name": "Safari"
        },
        "risk_level": "low",
        "requires_confirmation": false
    }
    "#;

    let parsed: IntentObject = serde_json::from_str(fake_llm_output)?;

    println!("Parsed Intent: {:?}", parsed);

    let validated = validate_parameters(&parsed)?;

    println!("Executing...\n");
    executor::system::execute(validated)?;
    println!("Done.");  

    Ok(())
}