// src/asr.rs — ASR 引擎（支持 Transducer 流式 / SenseVoice 离线）

use std::path::Path;
use anyhow::{Context, Result};

use sherpa_onnx::{
    OnlineRecognizer, OnlineRecognizerConfig, OnlineStream,
    OnlineModelConfig, OnlineTransducerModelConfig,
    OfflineRecognizer, OfflineRecognizerConfig,
    OfflineSenseVoiceModelConfig,
};

use crate::config::AppConfig;

/// ASR 引擎（双模式封装）
pub enum AsrEngine {
    /// 流式 Transducer（边说边解码，支持热词）
    Streaming {
        recg: OnlineRecognizer,
        config: AppConfig,
    },
    /// 离线 SenseVoice（一次性解码，更准确，自带标点）
    Offline {
        recg: OfflineRecognizer,
        config: AppConfig,
    },
}

impl AsrEngine {
    pub fn new(cfg: &AppConfig) -> Result<Self> {
        if cfg.is_sense_voice() {
            Self::new_sense_voice(cfg)
        } else {
            Self::new_transducer(cfg)
        }
    }

    fn new_transducer(cfg: &AppConfig) -> Result<Self> {
        let md = cfg.model_dir_str();

        let mut model_config = OnlineModelConfig::default();
        model_config.tokens = Some(cfg.tokens_file_str());
        model_config.num_threads = cfg.asr.n_threads;
        model_config.provider = Some("cpu".to_string());
        model_config.transducer = OnlineTransducerModelConfig {
            encoder: Some(format!("{}/{}", md, cfg.asr.encoder)),
            decoder: Some(format!("{}/{}", md, cfg.asr.decoder)),
            joiner:  Some(format!("{}/{}", md, cfg.asr.joiner)),
        };

        let mut rconfig = OnlineRecognizerConfig::default();
        rconfig.model_config = model_config;
        rconfig.decoding_method = Some("greedy_search".to_string());
        rconfig.enable_endpoint = true;
        rconfig.rule1_min_trailing_silence = 2.4;
        rconfig.rule2_min_trailing_silence = 1.2;
        rconfig.rule3_min_utterance_length = 20.0;

        if cfg.hotwords_file().exists() {
            if let Ok(bpe_path) = prepare_hotwords_bpe(&cfg.hotwords_file(), &cfg.model_dir()) {
                rconfig.decoding_method = Some("modified_beam_search".to_string());
                rconfig.max_active_paths = 6;
                rconfig.hotwords_file = Some(bpe_path);
                rconfig.hotwords_score = 3.0;
            }
        }

        let recg = OnlineRecognizer::create(&rconfig)
            .context("Transducer 创建失败")?;

        log::info!("ASR 就绪 | Transducer | 线程 {}", cfg.asr.n_threads);
        Ok(Self::Streaming { recg, config: cfg.clone() })
    }

    fn new_sense_voice(cfg: &AppConfig) -> Result<Self> {
        let md = cfg.model_dir_str();

        let mut rconfig = OfflineRecognizerConfig::default();
        rconfig.model_config.sense_voice = OfflineSenseVoiceModelConfig {
            model: Some(format!("{}/{}", md, cfg.asr.sense_voice_model)),
            language: Some("auto".into()),
            use_itn: true,
        };
        // SenseVoice 使用独立的 tokens 文件
        let sv_tokens = format!("{}/{}", md, cfg.asr.sense_voice_tokens);
        rconfig.model_config.tokens = Some(sv_tokens);
        rconfig.model_config.num_threads = cfg.asr.n_threads;
        rconfig.model_config.provider = Some("cpu".to_string());

        let recg = OfflineRecognizer::create(&rconfig)
            .context("SenseVoice 创建失败")?;

        log::info!("ASR 就绪 | SenseVoice | 线程 {}", cfg.asr.n_threads);
        Ok(Self::Offline { recg, config: cfg.clone() })
    }

    // ═══ 流式接口（Transducer 使用，SenseVoice 回退到攒音频）═══

    pub fn new_stream(&self) -> StreamHandle {
        match self {
            Self::Streaming { recg, .. } => StreamHandle::Online(recg.create_stream()),
            Self::Offline { .. } => StreamHandle::Buffer(Vec::new()),
        }
    }

    pub fn feed_and_decode(&self, handle: &mut StreamHandle, samples: &[f32]) {
        match (self, handle) {
            (Self::Streaming { recg, config }, StreamHandle::Online(stream)) => {
                stream.accept_waveform(config.audio.sample_rate as i32, samples);
                while recg.is_ready(stream) {
                    recg.decode(stream);
                }
            }
            (_, StreamHandle::Buffer(buf)) => {
                buf.extend_from_slice(samples);
            }
            _ => {}
        }
    }

    pub fn partial_result(&self, handle: &StreamHandle) -> String {
        match (self, handle) {
            (Self::Streaming { recg, .. }, StreamHandle::Online(stream)) => {
                recg.get_result(stream)
                    .map(|r| r.text.trim().to_string())
                    .unwrap_or_default()
            }
            (Self::Offline { .. }, StreamHandle::Buffer(buf)) => {
                if buf.is_empty() { String::new() }
                else { format!("录音中 ({:.1}s)...", buf.len() as f32 / 16000.0) }
            }
            _ => String::new(),
        }
    }

    pub fn finish_stream(&self, handle: &mut StreamHandle) -> String {
        match (self, handle) {
            (Self::Streaming { recg, .. }, StreamHandle::Online(stream)) => {
                stream.input_finished();
                while recg.is_ready(stream) {
                    recg.decode(stream);
                }
                recg.get_result(stream)
                    .map(|r| r.text.trim().to_string())
                    .unwrap_or_default()
            }
            (Self::Offline { recg, config }, StreamHandle::Buffer(buf)) => {
                if buf.is_empty() { return String::new(); }
                let stream = recg.create_stream();
                stream.accept_waveform(config.audio.sample_rate as i32, buf);
                recg.decode(&stream);
                stream.get_result()
                    .map(|r| r.text.trim().to_string())
                    .unwrap_or_default()
            }
            _ => String::new(),
        }
    }

    pub fn is_sense_voice(&self) -> bool {
        matches!(self, Self::Offline { .. })
    }
}

/// 流句柄（Online 用 OnlineStream，Offline 用音频缓冲区）
pub enum StreamHandle {
    Online(OnlineStream),
    Buffer(Vec<f32>),
}

// ═══ 热词 BPE 转换（仅 Transducer 使用）═══

fn prepare_hotwords_bpe(src: &Path, model_dir: &Path) -> Result<String> {
    let content = std::fs::read_to_string(src)?;
    let bpe_path = model_dir.join("_hotwords_bpe.txt");
    let mut output = String::new();
    let mut count = 0;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let (word, weight) = parse_hotword_line(line);
        if word.is_empty() { continue; }
        let bpe_tokens = word_to_bpe_tokens(&word);
        if bpe_tokens.is_empty() { continue; }
        if let Some(w) = weight {
            output.push_str(&format!("{} :{}\n", bpe_tokens, w));
        } else {
            output.push_str(&bpe_tokens);
            output.push('\n');
        }
        count += 1;
    }

    std::fs::write(&bpe_path, &output)?;
    log::info!("热词 BPE: {} 条", count);
    Ok(bpe_path.to_string_lossy().to_string())
}

fn parse_hotword_line(line: &str) -> (String, Option<String>) {
    let parts: Vec<&str> = line.rsplitn(2, ' ').collect();
    if parts.len() == 2 {
        let maybe_weight = parts[0].trim();
        if maybe_weight.parse::<f32>().is_ok() {
            return (parts[1].trim().to_string(), Some(maybe_weight.to_string()));
        }
    }
    (line.to_string(), None)
}

pub fn word_to_bpe_tokens(word: &str) -> String {
    word.chars()
        .filter(|c| !c.is_ascii_whitespace())
        .map(|ch| {
            if is_cjk(ch) { ch.to_string() }
            else if ch.is_ascii_alphanumeric() { ch.to_uppercase().to_string() }
            else { String::new() }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_cjk(c: char) -> bool {
    let cp = c as u32;
    (0x4E00..=0x9FFF).contains(&cp) || (0x3400..=0x4DBF).contains(&cp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_to_bpe_tokens() {
        assert_eq!(word_to_bpe_tokens("人工智能"), "人 工 智 能");
        assert_eq!(word_to_bpe_tokens("GitHub"), "G I T H U B");
        assert_eq!(word_to_bpe_tokens("AI模型"), "A I 模 型");
    }

    #[test]
    fn test_parse_hotword_line() {
        assert_eq!(parse_hotword_line("编程 6.0"), ("编程".into(), Some("6.0".into())));
        assert_eq!(parse_hotword_line("人工智能"), ("人工智能".into(), None));
        assert_eq!(parse_hotword_line("A股 2.5"), ("A股".into(), Some("2.5".into())));
    }
}