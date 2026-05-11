// src/asr.rs — Sherpa-ONNX 在线识别封装（流式接口）

use std::time::Instant;
use std::path::Path;
use anyhow::{Context, Result};

use sherpa_onnx::{
    OnlineRecognizer, OnlineRecognizerConfig, OnlineStream,
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
            match prepare_hotwords_bpe(&cfg.hotwords_file(), &cfg.model_dir()) {
                Ok(bpe_path) => {
                    rconfig.decoding_method = Some("modified_beam_search".to_string());
                    rconfig.max_active_paths = 4;
                    rconfig.hotwords_file = Some(bpe_path);
                    rconfig.hotwords_score = 2.0;
                }
                Err(e) => log::warn!("热词加载失败: {}", e),
            }
        }

        let recg = OnlineRecognizer::create(&rconfig)
            .context("创建识别器失败")?;

        log::info!("ASR 就绪 | 线程 {}", cfg.asr.n_threads);
        Ok(Self { config: cfg.clone(), recg })
    }

    // ═══ 流式接口 ═══

    /// 创建新的识别流（热键按下时调用）
    pub fn new_stream(&self) -> OnlineStream {
        self.recg.create_stream()
    }

    /// 喂入音频并解码（按住期间持续调用）
    pub fn feed_and_decode(&self, stream: &OnlineStream, samples: &[f32]) {
        stream.accept_waveform(self.config.audio.sample_rate as i32, samples);
        while self.recg.is_ready(stream) {
            self.recg.decode(stream);
        }
    }

    /// 获取中间结果（流式预览）
    pub fn partial_result(&self, stream: &OnlineStream) -> String {
        self.recg.get_result(stream)
            .map(|r| r.text.trim().to_string())
            .unwrap_or_default()
    }

    /// 结束并获取最终结果（松开热键时调用）
    pub fn finish_stream(&self, stream: &OnlineStream) -> String {
        stream.input_finished();
        while self.recg.is_ready(stream) {
            self.recg.decode(stream);
        }
        self.recg.get_result(stream)
            .map(|r| r.text.trim().to_string())
            .unwrap_or_default()
    }

    // ═══ 一次性接口（兼容旧代码）═══

    pub fn recognize(&self, samples: &[f32]) -> Result<RecogResult> {
        let t0 = Instant::now();
        if samples.is_empty() { return Ok(RecogResult::default()); }
        let stream = self.new_stream();
        self.feed_and_decode(&stream, samples);
        let text = self.finish_stream(&stream);
        Ok(RecogResult { text, latency_ms: t0.elapsed().as_millis() as u64 })
    }

    pub fn sample_rate(&self) -> i32 { self.config.audio.sample_rate as i32 }
}

// ═══ 热词 BPE 转换 ═══

fn prepare_hotwords_bpe(src: &Path, model_dir: &Path) -> Result<String> {
    let content = std::fs::read_to_string(src)?;
    let bpe_path = model_dir.join("_hotwords_bpe.txt");
    let mut output = String::new();
    let mut count = 0;

    for line in content.lines() {
        let word = line.trim();
        if word.is_empty() || word.starts_with('#') { continue; }
        let bpe_line = word_to_bpe_tokens(word);
        if !bpe_line.is_empty() {
            output.push_str(&bpe_line);
            output.push('\n');
            count += 1;
        }
    }

    std::fs::write(&bpe_path, &output)?;
    log::info!("热词 BPE: {} 条", count);
    Ok(bpe_path.to_string_lossy().to_string())
}

fn word_to_bpe_tokens(word: &str) -> String {
    let mut tokens: Vec<String> = Vec::new();
    for ch in word.chars() {
        if ch.is_ascii_whitespace() { continue; }
        if is_cjk(ch) { tokens.push(ch.to_string()); }
        else if ch.is_ascii_alphanumeric() { tokens.push(ch.to_uppercase().to_string()); }
    }
    tokens.join(" ")
}

fn is_cjk(c: char) -> bool {
    let cp = c as u32;
    (0x4E00..=0x9FFF).contains(&cp) || (0x3400..=0x4DBF).contains(&cp)
}