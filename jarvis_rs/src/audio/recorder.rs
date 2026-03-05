use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub fn record_wav(path: &str, seconds: u64) -> Result<(), Box<dyn std::error::Error>> {

    let host = cpal::default_host();

    let device = host
        .default_input_device()
        .expect("no input device available");

    let config = device.default_input_config()?;

    let sample_format = config.sample_format();
    let config: cpal::StreamConfig = config.into();

    let spec = hound::WavSpec {
        channels: config.channels,
        sample_rate: config.sample_rate.0,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let writer = Arc::new(Mutex::new(hound::WavWriter::create(path, spec)?));

    let writer_clone = writer.clone();

    let err_fn = |err| eprintln!("Stream error: {}", err);

    let stream = match sample_format {

        cpal::SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _| {
                let mut writer = writer_clone.lock().unwrap();
                for &sample in data {
                    let s = (sample * i16::MAX as f32) as i16;
                    writer.write_sample(s).ok();
                }
            },
            err_fn,
            None,
        )?,

        _ => panic!("Unsupported format"),
    };

    stream.play()?;

    thread::sleep(Duration::from_secs(seconds));

    drop(stream);

    writer.lock().unwrap().finalize()?;

    Ok(())
}