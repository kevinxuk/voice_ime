// src/config.rs — 配置管理（支持 Transducer / SenseVoice 双模型）

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub asr: AsrConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub hotkey: HotkeyConfig,
}

/// ASR 配置（支持 transducer / sense_voice 两种模型）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrConfig {
    pub n_threads: i32,
    /// "transducer" | "sense_voice"
    pub model_type: String,
    /// Transducer 模型文件（model_type = "transducer" 时使用）
    #[serde(default)]
    pub encoder: String,
    #[serde(default)]
    pub decoder: String,
    #[serde(default)]
    pub joiner: String,
    /// SenseVoice 模型文件（model_type = "sense_voice" 时使用）
    #[serde(default)]
    pub sense_voice_model: String,
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            n_threads: 4,
            model_type: "transducer".into(),
            encoder: "encoder-epoch-99-avg-1.int8.onnx".into(),
            decoder: "decoder-epoch-99-avg-1.onnx".into(),
            joiner: "joiner-epoch-99-avg-1.int8.onnx".into(),
            sense_voice_model: "model.int8.onnx".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: i32,
    pub vad_threshold: f32,
    pub silence_duration_ms: u64,
    pub min_speech_duration_ms: u64,
    pub buffer_frames: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            channels: 1,
            vad_threshold: 0.01,
            silence_duration_ms: 500,
            min_speech_duration_ms: 300,
            buffer_frames: 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub toggle: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self { toggle: "Ctrl+Alt+V".into() }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            asr: AsrConfig::default(),
            audio: AudioConfig::default(),
            hotkey: HotkeyConfig::default(),
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

    pub fn model_dir_str(&self) -> String { self.model_dir().to_string_lossy().to_string() }
    pub fn tokens_file(&self) -> PathBuf { self.model_dir().join("tokens.txt") }
    pub fn tokens_file_str(&self) -> String { self.tokens_file().to_string_lossy().to_string() }
    pub fn hotwords_file(&self) -> PathBuf { self.model_dir().join("hotwords.txt") }
    pub fn commands_file(&self) -> PathBuf { self.model_dir().join("commands.toml") }

    pub fn is_sense_voice(&self) -> bool {
        self.asr.model_type.to_lowercase() == "sense_voice"
            || self.asr.model_type.to_lowercase() == "sensevoice"
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        let dir = self.model_dir();
        if !dir.exists() { anyhow::bail!("模型目录不存在: {}", dir.display()); }
        if !self.tokens_file().exists() { anyhow::bail!("tokens.txt 不存在: {}", self.tokens_file().display()); }
        // 检查模型文件
        if self.is_sense_voice() {
            let m = dir.join(&self.asr.sense_voice_model);
            if !m.exists() { anyhow::bail!("SenseVoice 模型不存在: {}", m.display()); }
        } else {
            let e = dir.join(&self.asr.encoder);
            if !e.exists() { anyhow::bail!("encoder 不存在: {}", e.display()); }
        }
        Ok(())
    }
}