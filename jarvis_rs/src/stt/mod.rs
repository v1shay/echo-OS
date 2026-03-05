use std::process::Command;
use std::error::Error;
use std::fs;

pub struct WhisperSTT {
    pub model_path: String,
}

impl WhisperSTT {
    pub fn new(model_path: &str) -> Self {
        Self {
            model_path: model_path.to_string(),
        }
    }

    pub fn transcribe(&self, wav_path: &str) -> Result<String, Box<dyn Error>> {

        let output_prefix = "transcript";

        let status = Command::new("whisper-cli")
            .arg("-m")
            .arg(&self.model_path)
            .arg("-f")
            .arg(wav_path)
            .arg("-nt")
            .arg("-of")
            .arg(output_prefix)
            .status()?;

        if !status.success() {
            return Err("whisper-cli failed".into());
        }

        let transcript = fs::read_to_string("transcript.txt")?;

        Ok(transcript.trim().to_string())
    }
}