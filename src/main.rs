// src/main.rs — Voice IME 入口

mod config;
mod audio;
mod vad;
mod asr;
mod output;
mod learn;
mod ui;

use std::io::{self, Write};
use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::thread;

fn main() {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Info,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    ).ok();

    println!("========================================");
    println!("  Voice IME — 离线中文语音输入法");
    println!("========================================");

    let config = config::AppConfig::load_or_default();
    if let Err(e) = config.validate() {
        eprintln!("\n❌ {}\n", e);
        println!("请将模型文件放入 models/ 目录");
        println!("下载: https://github.com/k2-fsa/sherpa-onnx/releases");
        std::process::exit(1);
    }

    // ASR
    let asr = match asr::AsrEngine::new(&config) {
        Ok(e) => { log::info!("ASR 就绪"); e }
        Err(e) => { eprintln!("❌ ASR 失败: {}", e); std::process::exit(1); }
    };

    // 自学习
    let learner = learn::LearningEngine::new(&config.model_dir());

    // 输出
    let mut keyboard = output::KeyboardOutput::new();

    // VAD
    let mut vad = vad::VoiceDetect::new(&config.audio);

    // 全局退出信号
    let quit = Arc::new(AtomicBool::new(false));

    // UI 线程
    let ui_state = ui::UiState::new(quit.clone());
    let ui_energy = ui_state.energy.clone();
    let ui_quit = quit.clone();

    let ui_handle = thread::spawn(move || {
        ui::run_ui(ui_state);
    });

    // 音频采集
    let running = quit.clone();
    let (tx, rx) = mpsc::channel::<Vec<f32>>();
    let acfg = config.audio.clone();
    let audio_energy = ui_energy.clone();

    let _audio_h = thread::spawn(move || {
        let cap = audio::AudioCapture::new(acfg);
        struct Cb {
            tx: mpsc::Sender<Vec<f32>>,
            running: Arc<AtomicBool>,
            energy: Arc<std::sync::atomic::AtomicU8>,
        }
        impl audio::AudioCallback for Cb {
            fn on_audio_data(&mut self, s: &[f32], _: u32) {
                if self.running.load(Ordering::Relaxed) { return; } // quit=true 时停止
                // 计算能量并更新 UI
                let rms = vad::compute_energy(s);
                let level = (rms * 10.0).clamp(0.0, 1.0);
                self.energy.store((level * 255.0) as u8, Ordering::Relaxed);
                let _ = self.tx.send(s.to_vec());
            }
        }
        if let Err(e) = cap.start(Cb { tx, running, energy: audio_energy }) {
            log::error!("音频采集失败: {}", e);
        }
        loop { thread::sleep(Duration::from_secs(3600)); }
    });

    println!("✅ 就绪 | 右键窗口切换样式或退出\n");

    // 主循环
    loop {
        // 检查 UI 退出信号
        if quit.load(Ordering::Relaxed) {
            break;
        }

        while let Ok(samples) = rx.try_recv() {
            if samples.is_empty() { continue; }
            vad.feed(&samples);

            if vad.state() == vad::VadState::SpeechEnded {
                let speech = vad.get_speech().to_vec();
                if !speech.is_empty() {
                    match asr.recognize(&speech) {
                        Ok(r) if !r.text.is_empty() => {
                            let final_text = learner.process(&r.text);
                            if final_text != r.text {
                                log::info!("🎤 {} → {} ({}ms)", r.text, final_text, r.latency_ms);
                            } else {
                                log::info!("🎤 {} ({}ms)", final_text, r.latency_ms);
                            }
                            if let Err(e) = keyboard.send_text(&final_text) {
                                log::error!("输出失败: {}", e);
                            }
                        }
                        Ok(_) => {}
                        Err(e) => log::error!("识别: {}", e),
                    }
                }
                vad.reset();
            }
        }

        thread::sleep(Duration::from_millis(10));
    }

    // 退出清理
    learner.flush();
    let (vc, tf) = learner.stats();
    log::info!("📊 词频: {} 词, {} 次识别", vc, tf);
    let _ = ui_handle.join();
}