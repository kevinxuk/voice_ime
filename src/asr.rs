// src/asr.rs — Sherpa-ONNX 在线识别封装

use std::time::Instant;
use std::path::Path;
use anyhow::{Context, Result};

use sherpa_onnx::{
    OnlineRecognizer, OnlineRecognizerConfig,
    OnlineModelConfig, OnlineTransducerModelConfig,
};

use crate::config::AppConfig;

#[derive(Debug, Clone, Default)]
pub struct RecogResult {
    pub text: String,
    pub latency_ms: u64,
}

pub struct AsrEngine {
    config: AppConfig,
    recg: OnlineRecognizer,
}

impl AsrEngine {
    pub fn new(cfg: &AppConfig) -> Result<Self> {
        let md = cfg.model_dir_str();

        let mut model_config = OnlineModelConfig::default();
        model_config.tokens = Some(cfg.tokens_file_str());
        model_config.num_threads = cfg.asr.n_threads;
        model_config.provider = Some("cpu".to_string());
        model_config.transducer = OnlineTransducerModelConfig {
            encoder: Some(format!("{}/encoder-epoch-99-avg-1.int8.onnx", md)),
            decoder: Some(format!("{}/decoder-epoch-99-avg-1.onnx", md)),
            joiner:  Some(format!("{}/joiner-epoch-99-avg-1.int8.onnx", md)),
        };

        let mut rconfig = OnlineRecognizerConfig::default();
        rconfig.model_config = model_config;
        rconfig.decoding_method = Some("greedy_search".to_string());
        rconfig.enable_endpoint = true;
        rconfig.rule1_min_trailing_silence = 2.4;
        rconfig.rule2_min_trailing_silence = 1.2;
        rconfig.rule3_min_utterance_length = 20.0;

        if cfg.hotwords_file().exists() {
            // 将中文热词转换为 BPE 格式（每个字用空格分隔）
            // 用户 hotwords.txt 写 "人工智能"
            // 转换后 _hotwords_bpe.txt 写 "人 工 智 能"
            match prepare_hotwords_bpe(&cfg.hotwords_file(), &cfg.model_dir()) {
                Ok(bpe_path) => {
                    rconfig.decoding_method = Some("modified_beam_search".to_string());
                    rconfig.max_active_paths = 4;
                    rconfig.hotwords_file = Some(bpe_path);
                    rconfig.hotwords_score = 2.0;
                }
                Err(e) => {
                    log::warn!("热词加载失败: {}，使用 greedy_search", e);
                }
            }
        }

        let recg = OnlineRecognizer::create(&rconfig)
            .context("创建识别器失败，请检查 models/ 下的模型文件和 tokens.txt")?;

        log::info!("ASR 就绪 | 线程 {}", cfg.asr.n_threads);
        Ok(Self { config: cfg.clone(), recg })
    }

    /// 识别完整音频段
    pub fn recognize(&self, samples: &[f32]) -> Result<RecogResult> {
        let t0 = Instant::now();
        if samples.is_empty() {
            return Ok(RecogResult::default());
        }

        let sr = self.config.audio.sample_rate as i32;
        let stream = self.recg.create_stream();
        stream.accept_waveform(sr, samples);
        stream.input_finished();

        while self.recg.is_ready(&stream) {
            self.recg.decode(&stream);
        }

        let text = self.recg.get_result(&stream)
            .map(|r| r.text.trim().to_string())
            .unwrap_or_default();

        Ok(RecogResult {
            text,
            latency_ms: t0.elapsed().as_millis() as u64,
        })
    }
}

/// 将用户友好的 hotwords.txt 转换为 BPE 格式
/// "人工智能" → "人 工 智 能"
/// "GitHub" → "▁G IT H U B" (英文加 ▁ 前缀，按字母拆)
fn prepare_hotwords_bpe(src: &Path, model_dir: &Path) -> Result<String> {
    let content = std::fs::read_to_string(src)?;
    let bpe_path = model_dir.join("_hotwords_bpe.txt");
    let mut output = String::new();
    let mut count = 0;

    for line in content.lines() {
        let word = line.trim();
        if word.is_empty() || word.starts_with('#') || word.starts_with("//") {
            continue;
        }

        // 将每个字符用空格分隔
        let bpe_line = word_to_bpe_tokens(word);
        if !bpe_line.is_empty() {
            output.push_str(&bpe_line);
            output.push('\n');
            count += 1;
        }
    }

    std::fs::write(&bpe_path, &output)?;
    log::info!("热词 BPE 转换完成: {} 条 → {}", count, bpe_path.display());
    Ok(bpe_path.to_string_lossy().to_string())
}

/// 将一个词转为空格分隔的 token 序列
/// 中文: "人工智能" → "人 工 智 能"
/// 英文: "GitHub" → "▁G I T H U B" (BPE 模型的英文通常以 ▁ 开头)
fn word_to_bpe_tokens(word: &str) -> String {
    let mut tokens: Vec<String> = Vec::new();

    for ch in word.chars() {
        if ch.is_ascii_whitespace() {
            continue;
        }
        // 中文字符：直接作为一个 token
        if is_cjk(ch) {
            tokens.push(ch.to_string());
        }
        // ASCII 字母/数字：大写化（这个模型 tokens 里英文是大写）
        else if ch.is_ascii_alphanumeric() {
            tokens.push(ch.to_uppercase().to_string());
        }
        // 其他字符跳过
    }

    tokens.join(" ")
}

fn is_cjk(c: char) -> bool {
    let cp = c as u32;
    (0x4E00..=0x9FFF).contains(&cp)     // CJK Unified
    || (0x3400..=0x4DBF).contains(&cp)   // CJK Extension A
    || (0x20000..=0x2A6DF).contains(&cp) // CJK Extension B
    || (0xF900..=0xFAFF).contains(&cp)   // CJK Compatibility
}