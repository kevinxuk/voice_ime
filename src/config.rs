// src/config.rs

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub asr: AsrConfig,
    pub audio: AudioConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrConfig {
    pub n_threads: i32,
    pub decoding_method: String,
    pub model_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: i32,
    pub vad_threshold: f32,
    pub silence_duration_ms: u64,
    pub min_speech_duration_ms: u64,
    pub buffer_frames: u32,
    pub use_vad_endpoint: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            asr: AsrConfig {
                n_threads: 4,
                decoding_method: "greedy_search".into(),
                model_type: "transducer".into(),
            },
            audio: AudioConfig {
                sample_rate: 16000,
                channels: 1,
                vad_threshold: 0.01,
                silence_duration_ms: 500,
                min_speech_duration_ms: 300,
                buffer_frames: 1024,
                use_vad_endpoint: false,
            },
        }
    }
}

impl AppConfig {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }

    pub fn load_or_default() -> Self {
        match Self::load("voice_ime.toml") {
            Ok(c) => { log::info!("已加载 voice_ime.toml"); c }
            Err(_) => Self::default(),
        }
    }

    pub fn model_dir(&self) -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("models")
    }

    pub fn model_dir_str(&self) -> String {
        self.model_dir().to_string_lossy().to_string()
    }

    pub fn tokens_file(&self) -> PathBuf { self.model_dir().join("tokens.txt") }
    pub fn tokens_file_str(&self) -> String { self.tokens_file().to_string_lossy().to_string() }
    pub fn hotwords_file(&self) -> PathBuf { self.model_dir().join("hotwords.txt") }
    pub fn hotwords_file_str(&self) -> String { self.hotwords_file().to_string_lossy().to_string() }

    pub fn validate(&self) -> anyhow::Result<()> {
        let dir = self.model_dir();
        if !dir.exists() {
            anyhow::bail!("模型目录不存在: {}", dir.display());
        }
        let tokens = self.tokens_file();
        if !tokens.exists() {
            anyhow::bail!("tokens.txt 不存在: {}", tokens.display());
        }
        Ok(())
    }
}