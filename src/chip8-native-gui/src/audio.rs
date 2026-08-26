use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

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

pub type SharedAudio = Arc<AudioState>;

/// Shared state read by the real-time audio callback.
///
/// The callback must not wait for the emulation thread: even a short mutex
/// contention can make the audio device run out of samples. Individual atomic
/// loads can observe adjacent emulator states, which is harmless for this
/// simple waveform and preferable to blocking the callback.
pub struct AudioState {
    pattern: [AtomicU8; 16],
    pitch: AtomicU8,
    active: AtomicBool,
}

impl AudioState {
    pub(crate) fn snapshot(&self) -> AudioSnapshot {
        AudioSnapshot {
            pattern: self
                .pattern
                .each_ref()
                .map(|bit| bit.load(Ordering::Relaxed)),
            pitch: self.pitch.load(Ordering::Relaxed),
            active: self.active.load(Ordering::Relaxed),
        }
    }

    pub fn update(&self, snapshot: AudioSnapshot) {
        for (target, value) in self.pattern.iter().zip(snapshot.pattern) {
            target.store(value, Ordering::Relaxed);
        }
        self.pitch.store(snapshot.pitch, Ordering::Relaxed);
        self.active.store(snapshot.active, Ordering::Relaxed);
    }
}

pub fn shared_state() -> SharedAudio {
    let snapshot = AudioSnapshot::default();
    Arc::new(AudioState {
        pattern: snapshot.pattern.map(AtomicU8::new),
        pitch: AtomicU8::new(snapshot.pitch),
        active: AtomicBool::new(snapshot.active),
    })
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
        let sample_rate = config.sample_rate() as f32;
        let channels = usize::from(config.channels());
        let mut phase = 0.0;
        let callback = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            fill_samples(data, channels, sample_rate, state.snapshot(), &mut phase);
        };
        if config.sample_format() != cpal::SampleFormat::F32 {
            return Err(format!(
                "unsupported output sample format: {:?}",
                config.sample_format()
            ));
        }
        let stream = device
            .build_output_stream(
                config.into(),
                callback,
                |error| {
                    // CPAL 0.18 may report an Xrun while CoreAudio primes a new
                    // output stream. The stream recovers automatically, so only
                    // report errors that require user attention.
                    if error.kind() != cpal::ErrorKind::Xrun {
                        eprintln!("audio stream error: {error}");
                    }
                },
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

    #[test]
    fn shared_state_returns_the_latest_snapshot() {
        let state = shared_state();
        let snapshot = AudioSnapshot {
            pattern: [0xA5; 16],
            pitch: 90,
            active: true,
        };

        state.update(snapshot);

        assert_eq!(state.snapshot().pattern, snapshot.pattern);
        assert_eq!(state.snapshot().pitch, snapshot.pitch);
        assert!(state.snapshot().active);
    }
}
