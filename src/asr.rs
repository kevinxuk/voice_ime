// src/asr.rs — Sherpa-ONNX 在线识别封装（最小版本）

use std::time::Instant;
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
            // 热词要求 modified_beam_search
            rconfig.decoding_method = Some("modified_beam_search".to_string());
            rconfig.max_active_paths = 4;
            rconfig.hotwords_file = Some(cfg.hotwords_file_str());
            rconfig.hotwords_score = 2.0;
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