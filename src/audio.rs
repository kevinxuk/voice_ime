// src/audio.rs — cpal 音频采集

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use log::error;

use crate::config::AudioConfig;

/// 音频数据回调
pub trait AudioCallback: Send + 'static {
    fn on_audio_data(&mut self, samples: &[f32], sample_rate: u32);
}

/// 音频采集器
pub struct AudioCapture {
    config: AudioConfig,
}

impl AudioCapture {
    pub fn new(config: AudioConfig) -> Self {
        let host = cpal::default_host();
        match host.default_input_device() {
            Some(ref dev) => log::info!("🎤 音频设备: {}", dev.name().unwrap_or_default()),
            None => log::warn!("⚠ 未找到音频输入设备"),
        }
        Self { config }
    }

    /// 开始采集音频，回调在音频线程中调用
    pub fn start<C>(&self, callback: C) -> anyhow::Result<()>
    where
        C: AudioCallback + 'static,
    {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("无默认音频输入设备"))?;

        // 使用设备默认配置（避免 "not supported" 错误）
        let default_fmt = device.default_input_config()?;
        let device_sample_rate = default_fmt.sample_rate().0;
        let device_channels = default_fmt.channels();

        let stream_config = cpal::StreamConfig {
            channels: device_channels,
            sample_rate: cpal::SampleRate(device_sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let target_sample_rate = self.config.sample_rate; // 16000
        let err_fn = |err: cpal::StreamError| error!("音频流错误: {}", err);
        let callback = Arc::new(Mutex::new(callback));

        log::info!("设备格式: {}Hz {}ch {:?} → 目标 {}Hz",
            device_sample_rate, device_channels, default_fmt.sample_format(), target_sample_rate);

        let stream = match default_fmt.sample_format() {
            cpal::SampleFormat::F32 => Self::build_f32(
                &device, &stream_config, target_sample_rate, device_sample_rate, device_channels, callback, err_fn,
            )?,
            cpal::SampleFormat::I16 => Self::build_i16(
                &device, &stream_config, target_sample_rate, device_sample_rate, device_channels, callback, err_fn,
            )?,
            _ => anyhow::bail!("不支持的采样格式: {:?}", default_fmt.sample_format()),
        };

        stream.play()?;
        log::info!("▶ 音频采集已启动");
        std::mem::forget(stream);
        Ok(())
    }

    fn build_f32(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        target_rate: u32,
        device_rate: u32,
        device_channels: u16,
        callback: Arc<Mutex<dyn AudioCallback>>,
        err_fn: impl Fn(cpal::StreamError) + Send + 'static,
    ) -> anyhow::Result<cpal::Stream> {
        device.build_input_stream(
            config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                // 1. 混合到单声道
                let mono = to_mono(data, device_channels as usize);
                // 2. 重采样到目标采样率
                let resampled = resample(&mono, device_rate, target_rate);
                callback.lock().unwrap().on_audio_data(&resampled, target_rate);
            },
            err_fn,
            None,
        ).map_err(|e| e.into())
    }

    fn build_i16(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        target_rate: u32,
        device_rate: u32,
        device_channels: u16,
        callback: Arc<Mutex<dyn AudioCallback>>,
        err_fn: impl Fn(cpal::StreamError) + Send + 'static,
    ) -> anyhow::Result<cpal::Stream> {
        device.build_input_stream(
            config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let f32_data: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                let mono = to_mono(&f32_data, device_channels as usize);
                let resampled = resample(&mono, device_rate, target_rate);
                callback.lock().unwrap().on_audio_data(&resampled, target_rate);
            },
            err_fn,
            None,
        ).map_err(|e| e.into())
    }
}

/// 多声道 → 单声道（取平均）
fn to_mono(data: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return data.to_vec();
    }
    data.chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// 简单线性重采样（低通滤波+抽样）
fn resample(data: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return data.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = (data.len() as f64 / ratio).ceil() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_idx = i as f64 * ratio;
        let idx = src_idx as usize;
        let frac = src_idx - idx as f64;
        let s = if idx + 1 < data.len() {
            data[idx] * (1.0 - frac as f32) + data[idx + 1] * frac as f32
        } else if idx < data.len() {
            data[idx]
        } else {
            0.0
        };
        out.push(s);
    }
    out
}

/// 计算音频 RMS 能量（从 vad.rs 迁移过来）
pub fn compute_energy(samples: &[f32]) -> f32 {
    if samples.is_empty() { return 0.0; }
    let sq: f32 = samples.iter().map(|s| s * s).sum();
    (sq / samples.len() as f32).sqrt()
}