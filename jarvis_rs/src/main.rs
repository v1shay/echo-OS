mod intent;
mod executor;
mod llm;

use anyhow::Result;
use intent::*;
use llm::ollama_client::OllamaClient;
use llm::prompt::build_intent_prompt;
use std::io::{self, Write};

#[tokio::main]
async fn main() -> Result<()> {

    let client = OllamaClient::new();

    loop {
        print!("Jarvis > ");
        io::stdout().flush()?;

        let mut user_input = String::new();
        io::stdin().read_line(&mut user_input)?;
        let user_input = user_input.trim();

        if user_input == "exit" {
            break;
        }

        let prompt = build_intent_prompt(user_input);

        println!("Processing...\n");

        let raw_output = client.generate(&prompt).await?;

        println!("Raw LLM Output:\n{}\n", raw_output);

        let parsed: IntentObject = serde_json::from_str(&raw_output)?;

        println!("Parsed Intent: {:?}\n", parsed);

        let validated = validate_parameters(&parsed)?;

        println!("Executing...\n");
        executor::system::execute(validated)?;

        println!("Done.\n");
    }

    Ok(())
}