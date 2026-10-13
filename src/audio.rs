//! Audio output — plays the game's Creative Voice samples through the OS
//! mixer (feature `audio`, built on `cpal`).
//!
//! The DOS driver is single-voice: a new sample cuts the previous one (driver
//! cmd 8 = stop before cmd 6 = play), which the mixer reproduces.

#[cfg(feature = "audio")]
mod audio_impl {
    use crate::sound::Voc;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::{Arc, Mutex};

    struct Voice {
        data: Vec<f32>,
        pos: usize,
    }

    /// Single-voice mixer.
    #[derive(Default)]
    pub struct Mixer {
        voice: Option<Voice>,
    }

    impl Mixer {
        /// Start a sample, cutting the current one (DOS driver behaviour).
        pub fn play(&mut self, voc: &Voc, out_rate: u32) {
            let data: Vec<f32> = voc
                .resample(out_rate)
                .into_iter()
                .map(|s| s as f32 / 32768.0)
                .collect();
            self.voice = Some(Voice { data, pos: 0 });
        }

        /// Whether the voice has finished (debug/tests).
        pub fn is_idle(&self) -> bool {
            self.voice.is_none()
        }

        /// Fill one mono buffer; silence when idle (engine single-voice mix).
        pub fn fill(&mut self, out: &mut [f32]) {
            out.fill(0.0);
            if let Some(voice) = &mut self.voice {
                for slot in out.iter_mut() {
                    if voice.pos >= voice.data.len() {
                        break;
                    }
                    *slot = voice.data[voice.pos];
                    voice.pos += 1;
                }
                if voice.pos >= voice.data.len() {
                    self.voice = None;
                }
            }
        }
    }

    /// Live output stream plus its mixer.
    pub struct AudioOut {
        mixer: Arc<Mutex<Mixer>>,
        rate: u32,
        _stream: cpal::Stream,
    }

    impl AudioOut {
        /// Open the default output device, or `None` when unavailable.
        pub fn new() -> Option<Self> {
            let host = cpal::default_host();
            let device = host.default_output_device()?;
            let supported = device.default_output_config().ok()?;
            let format = supported.sample_format();
            let config = supported.config();
            let rate = config.sample_rate;
            let channels = config.channels as usize;
            let mixer = Arc::new(Mutex::new(Mixer::default()));
            let shared = mixer.clone();
            let fill = move |data: &mut [f32]| {
                let frames = data.len() / channels.max(1);
                let mut mono = vec![0.0f32; frames];
                if let Ok(mut mixer) = shared.lock() {
                    mixer.fill(&mut mono);
                }
                for (i, frame) in data.chunks_mut(channels.max(1)).enumerate() {
                    for sample in frame.iter_mut() {
                        *sample = mono.get(i).copied().unwrap_or(0.0);
                    }
                }
            };
            let stream = match format {
                cpal::SampleFormat::F32 => device.build_output_stream(
                    config,
                    move |data: &mut [f32], _| fill(data),
                    |err| eprintln!("audio stream error: {err}"),
                    None,
                ),
                cpal::SampleFormat::I16 => {
                    let shared = mixer.clone();
                    device.build_output_stream(
                        config,
                        move |data: &mut [i16], _| {
                            let frames = data.len() / channels.max(1);
                            let mut mono = vec![0.0f32; frames];
                            if let Ok(mut mixer) = shared.lock() {
                                mixer.fill(&mut mono);
                            }
                            for (i, frame) in data.chunks_mut(channels.max(1)).enumerate() {
                                let v = mono.get(i).copied().unwrap_or(0.0);
                                let sample = (v.clamp(-1.0, 1.0) * 32767.0) as i16;
                                for s in frame.iter_mut() {
                                    *s = sample;
                                }
                            }
                        },
                        |err| eprintln!("audio stream error: {err}"),
                        None,
                    )
                }
                _ => return None,
            }
            .ok()?;
            stream.play().ok()?;
            Some(AudioOut {
                mixer,
                rate,
                _stream: stream,
            })
        }

        /// Play a sample (cuts the previous one).
        pub fn play_voc(&self, voc: &Voc) {
            if let Ok(mut mixer) = self.mixer.lock() {
                mixer.play(voc, self.rate);
            }
        }
    }
}

#[cfg(feature = "audio")]
pub use audio_impl::{AudioOut, Mixer};

/// Stub used by builds without the `audio` feature.
#[cfg(not(feature = "audio"))]
pub struct AudioOut;

#[cfg(not(feature = "audio"))]
impl AudioOut {
    /// No output device in builds without audio.
    pub fn new() -> Option<Self> {
        None
    }

    /// No-op.
    pub fn play_voc(&self, _voc: &crate::sound::Voc) {}
}

#[cfg(all(test, feature = "audio"))]
mod tests {
    use super::*;
    use crate::sound::Voc;

    #[test]
    fn mixer_plays_and_cuts() {
        let mut mixer = Mixer::default();
        let voc = Voc {
            name: "test".into(),
            rate_hz: 8000,
            samples: vec![128; 800],
            blocks: 1,
        };
        mixer.play(&voc, 8000);
        let mut buf = vec![0.0f32; 100];
        mixer.fill(&mut buf);
        // Silence (0x80) maps to 0.0; the voice must still be active.
        assert!(!mixer.is_idle());
        // A second sample cuts the first.
        mixer.play(&voc, 8000);
        let mut buf2 = vec![0.0f32; 2000];
        mixer.fill(&mut buf2);
        assert!(mixer.is_idle(), "sample played to completion");
    }
}
