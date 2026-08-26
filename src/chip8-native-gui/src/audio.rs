use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

#[derive(Clone, Copy)]
pub struct AudioSnapshot {
    pub pattern: [u8; 16],
    pub pitch: u8,
    pub active: bool,
}

impl Default for AudioSnapshot {
    fn default() -> Self {
        Self {
            pattern: [0; 16],
            pitch: 64,
            active: false,
        }
    }
}

pub type SharedAudio = Arc<Mutex<AudioSnapshot>>;

pub fn shared_state() -> SharedAudio {
    Arc::new(Mutex::new(AudioSnapshot::default()))
}

pub struct AudioOutput {
    _stream: cpal::Stream,
}

impl AudioOutput {
    pub fn open(state: SharedAudio) -> Result<Self, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("no default audio output device")?;
        let config = device
            .default_output_config()
            .map_err(|error| error.to_string())?;
        let sample_rate = config.sample_rate().0 as f32;
        let channels = usize::from(config.channels());
        let mut phase = 0.0;
        let callback = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let snapshot = state.lock().map(|state| *state).unwrap_or_default();
            fill_samples(data, channels, sample_rate, snapshot, &mut phase);
        };
        if config.sample_format() != cpal::SampleFormat::F32 {
            return Err(format!(
                "unsupported output sample format: {:?}",
                config.sample_format()
            ));
        }
        let stream = device
            .build_output_stream(
                &config.into(),
                callback,
                |error| eprintln!("audio stream error: {error}"),
                None,
            )
            .map_err(|error| error.to_string())?;
        stream.play().map_err(|error| error.to_string())?;
        Ok(Self { _stream: stream })
    }
}

pub fn fill_samples(
    samples: &mut [f32],
    channels: usize,
    sample_rate: f32,
    state: AudioSnapshot,
    phase: &mut f32,
) {
    if !state.active {
        samples.fill(0.0);
        *phase = 0.0;
        return;
    }
    let frequency = 4_000.0 * 2.0_f32.powf((f32::from(state.pitch) - 64.0) / 48.0);
    for frame in samples.chunks_exact_mut(channels) {
        let bit = (*phase as usize) % 128;
        let value = if state.pattern[bit / 8] & (0x80 >> (bit % 8)) != 0 {
            0.20
        } else {
            -0.20
        };
        frame.fill(value);
        *phase += frequency / sample_rate;
        if *phase >= 128.0 {
            *phase -= 128.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inactive_audio_is_silent_and_resets_phase() {
        let mut samples = [1.0; 2];
        let mut phase = 42.0;
        fill_samples(
            &mut samples,
            1,
            48_000.0,
            AudioSnapshot::default(),
            &mut phase,
        );
        assert_eq!(samples, [0.0; 2]);
        assert_eq!(phase, 0.0);
    }
}
