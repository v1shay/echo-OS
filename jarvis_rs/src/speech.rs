use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[async_trait]
pub trait SpeechToText: Send + Sync {
    async fn record_and_transcribe(&self, output_path: &Path, seconds: u64) -> Result<String>;
}

pub trait TextToSpeech: Send + Sync {
    fn speak(&self, text: &str) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct WhisperCommandSpeechToText {
    model_path: Option<PathBuf>,
}

impl WhisperCommandSpeechToText {
    pub fn new(model_path: Option<PathBuf>) -> Self {
        Self { model_path }
    }
}

#[async_trait]
impl SpeechToText for WhisperCommandSpeechToText {
    async fn record_and_transcribe(&self, output_path: &Path, seconds: u64) -> Result<String> {
        let output_path = output_path.to_path_buf();
        let model_path = self.model_path.clone();

        tokio::task::spawn_blocking(move || {
            record_wav(&output_path, seconds)?;
            transcribe_with_whisper_cli(model_path.as_deref(), &output_path)
        })
        .await
        .context("speech-to-text worker join error")?
    }
}

#[derive(Debug, Default)]
pub struct NoopTextToSpeech;

impl TextToSpeech for NoopTextToSpeech {
    fn speak(&self, _text: &str) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct MacOsSayTextToSpeech {
    voice_name: String,
}

impl MacOsSayTextToSpeech {
    pub fn new(voice_name: String) -> Self {
        Self { voice_name }
    }
}

impl TextToSpeech for MacOsSayTextToSpeech {
    fn speak(&self, text: &str) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            Command::new("say")
                .arg("-v")
                .arg(&self.voice_name)
                .arg(text)
                .spawn()
                .context("failed to start macOS say command")?;
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (&self.voice_name, text);
        }
        Ok(())
    }
}

pub fn record_wav(path: &Path, seconds: u64) -> Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .context("no input audio device available")?;
    let config = device.default_input_config()?;
    let sample_format = config.sample_format();
    let stream_config: cpal::StreamConfig = config.clone().into();

    let spec = hound::WavSpec {
        channels: stream_config.channels,
        sample_rate: stream_config.sample_rate.0,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let writer = Arc::new(Mutex::new(Some(hound::WavWriter::create(path, spec)?)));
    let writer_clone = Arc::clone(&writer);
    let err_fn = |err| eprintln!("audio stream error: {}", err);

    let stream = match sample_format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &stream_config,
            move |data: &[f32], _| write_samples_f32(&writer_clone, data),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_input_stream(
            &stream_config,
            move |data: &[i16], _| write_samples_i16(&writer_clone, data),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::U16 => device.build_input_stream(
            &stream_config,
            move |data: &[u16], _| write_samples_u16(&writer_clone, data),
            err_fn,
            None,
        )?,
        other => bail!("unsupported input sample format: {:?}", other),
    };

    stream.play()?;
    thread::sleep(Duration::from_secs(seconds));
    drop(stream);

    if let Some(writer) = writer.lock().expect("wav writer poisoned").take() {
        writer.finalize()?;
    }

    Ok(())
}

fn write_samples_f32(writer: &Arc<Mutex<Option<hound::WavWriter<std::io::BufWriter<std::fs::File>>>>>, data: &[f32]) {
    if let Ok(mut writer) = writer.lock() {
        if let Some(writer) = writer.as_mut() {
            for sample in data {
                let value = (sample * i16::MAX as f32) as i16;
                let _ = writer.write_sample(value);
            }
        }
    }
}

fn write_samples_i16(writer: &Arc<Mutex<Option<hound::WavWriter<std::io::BufWriter<std::fs::File>>>>>, data: &[i16]) {
    if let Ok(mut writer) = writer.lock() {
        if let Some(writer) = writer.as_mut() {
            for sample in data {
                let _ = writer.write_sample(*sample);
            }
        }
    }
}

fn write_samples_u16(writer: &Arc<Mutex<Option<hound::WavWriter<std::io::BufWriter<std::fs::File>>>>>, data: &[u16]) {
    if let Ok(mut writer) = writer.lock() {
        if let Some(writer) = writer.as_mut() {
            for sample in data {
                let value = (*sample as i32 - i16::MAX as i32) as i16;
                let _ = writer.write_sample(value);
            }
        }
    }
}

fn transcribe_with_whisper_cli(model_path: Option<&Path>, wav_path: &Path) -> Result<String> {
    let output_prefix = wav_path.with_extension("");
    let transcript_path = output_prefix.with_extension("txt");
    let mut command = Command::new("whisper-cli");
    if let Some(model_path) = model_path {
        command.arg("-m").arg(model_path);
    }
    command
        .arg("-f")
        .arg(wav_path)
        .arg("-nt")
        .arg("-of")
        .arg(output_prefix.as_os_str());

    let status = command.status().context("failed to start whisper-cli")?;
    if !status.success() {
        bail!("whisper-cli exited with status {}", status);
    }

    let transcript = fs::read_to_string(&transcript_path)
        .with_context(|| format!("failed to read {}", transcript_path.display()))?;
    Ok(transcript.trim().to_string())
}
