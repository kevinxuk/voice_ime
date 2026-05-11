// src/vad.rs — 纯能量 VAD（无需额外模型文件）

use crate::config::AudioConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadState {
    Silence,
    SpeechStarted,
    Speaking,
    SpeechEnded,
}

pub struct VoiceDetect {
    config: AudioConfig,
    threshold: f32,
    silence_count: usize,
    speech_count: usize,
    state: VadState,
    speech_start: Option<std::time::Instant>,
    audio_buffer: Vec<f32>,
}

impl VoiceDetect {
    pub fn new(config: &AudioConfig) -> Self {
        Self {
            config: config.clone(),
            threshold: config.vad_threshold,
            silence_count: 0,
            speech_count: 0,
            state: VadState::Silence,
            speech_start: None,
            audio_buffer: Vec::new(),
        }
    }

    pub fn process(&mut self, samples: &[f32]) -> Option<VadState> {
        let energy = compute_energy(samples);
        let frame_ms = samples.len() as f64 / self.config.sample_rate as f64 * 1000.0;

        self.audio_buffer.extend(samples);
        let max_buf = self.config.sample_rate as usize * 120;
        if self.audio_buffer.len() > max_buf {
            self.audio_buffer.drain(..self.audio_buffer.len() - max_buf);
        }

        match self.state {
            VadState::Silence | VadState::SpeechEnded => {
                if energy > self.threshold {
                    self.speech_count += 1;
                    let min_frames = ((self.config.min_speech_duration_ms as f64 / frame_ms).ceil() as usize).max(1);
                    if self.speech_count >= min_frames {
                        self.state = VadState::SpeechStarted;
                        self.speech_start = Some(std::time::Instant::now());
                        self.silence_count = 0;
                        return Some(VadState::SpeechStarted);
                    }
                } else {
                    self.speech_count = 0;
                }
            }
            VadState::SpeechStarted | VadState::Speaking => {
                if energy > self.threshold {
                    self.silence_count = 0;
                    self.state = VadState::Speaking;
                } else {
                    self.silence_count += 1;
                    let silence_frames = ((self.config.silence_duration_ms as f64 / frame_ms).ceil() as usize).max(1);
                    if self.silence_count >= silence_frames {
                        let dur = self.speech_start.map(|t| t.elapsed().as_millis() as u64).unwrap_or(0);
                        if dur >= self.config.min_speech_duration_ms {
                            self.state = VadState::SpeechEnded;
                            self.silence_count = 0;
                            self.speech_count = 0;
                            self.speech_start = None;
                            return Some(VadState::SpeechEnded);
                        }
                    }
                }
            }
        }
        None
    }

    pub fn feed(&mut self, samples: &[f32]) { let _ = self.process(samples); }
    pub fn state(&self) -> VadState { self.state }
    pub fn get_speech(&self) -> &[f32] { &self.audio_buffer }

    pub fn reset(&mut self) {
        self.silence_count = 0;
        self.speech_count = 0;
        self.state = VadState::Silence;
        self.speech_start = None;
        self.audio_buffer.clear();
    }
}

impl Default for VoiceDetect {
    fn default() -> Self { Self::new(&AudioConfig::default()) }
}

fn compute_energy(samples: &[f32]) -> f32 {
    if samples.is_empty() { return 0.0; }
    let sq: f32 = samples.iter().map(|s| s * s).sum();
    (sq / samples.len() as f32).sqrt()
}