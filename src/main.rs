// src/main.rs — Voice IME 入口
// 启动流程: UI窗口(加载进度) → 模型初始化 → 就绪 → 语音识别

mod config;
mod audio;
mod vad;
mod asr;
mod output;
mod learn;
mod ui;

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

    // ═══ 1. 先启动 UI 窗口（显示加载进度） ═══
    let quit = Arc::new(AtomicBool::new(false));
    let ui_state = ui::UiState::new(quit.clone());
    let ui_energy = ui_state.energy.clone();
    let ui_phase = ui_state.phase.clone();
    let ui_progress = ui_state.progress.clone();

    // UI 阶段: 加载中
    ui_state.set_phase_loading();
    ui_state.set_progress(0);

    let ui_handle = thread::spawn(move || {
        ui::run_ui(ui_state);
    });

    // ═══ 2. 加载配置 ═══
    ui_progress.store(10, Ordering::Relaxed);
    let config = config::AppConfig::load_or_default();
    if let Err(e) = config.validate() {
        log::error!("配置错误: {}", e);
        eprintln!("❌ {}\n请将模型放入 models/ 目录", e);
        quit.store(true, Ordering::Relaxed);
        let _ = ui_handle.join();
        std::process::exit(1);
    }
    ui_progress.store(20, Ordering::Relaxed);
    log::info!("配置加载完成");

    // ═══ 3. 加载 ASR 模型（最耗时） ═══
    ui_progress.store(30, Ordering::Relaxed);
    let asr = match asr::AsrEngine::new(&config) {
        Ok(e) => {
            ui_progress.store(80, Ordering::Relaxed);
            log::info!("ASR 模型加载完成");
            e
        }
        Err(e) => {
            log::error!("ASR 失败: {}", e);
            eprintln!("❌ ASR: {}", e);
            quit.store(true, Ordering::Relaxed);
            let _ = ui_handle.join();
            std::process::exit(1);
        }
    };

    // ═══ 4. 初始化其他模块 ═══
    ui_progress.store(85, Ordering::Relaxed);
    let learner = learn::LearningEngine::new(&config.model_dir());

    ui_progress.store(90, Ordering::Relaxed);
    let mut keyboard = output::KeyboardOutput::new();
    let mut vad = vad::VoiceDetect::new(&config.audio);

    // ═══ 5. 启动音频采集 ═══
    ui_progress.store(95, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel::<Vec<f32>>();
    let acfg = config.audio.clone();
    let audio_energy = ui_energy.clone();
    let audio_quit = quit.clone();

    let _audio_h = thread::spawn(move || {
        let cap = audio::AudioCapture::new(acfg);
        struct Cb {
            tx: mpsc::Sender<Vec<f32>>,
            quit: Arc<AtomicBool>,
            energy: Arc<std::sync::atomic::AtomicU8>,
        }
        impl audio::AudioCallback for Cb {
            fn on_audio_data(&mut self, s: &[f32], _: u32) {
                if self.quit.load(Ordering::Relaxed) { return; }
                let rms = vad::compute_energy(s);
                let level = (rms * 10.0).clamp(0.0, 1.0);
                self.energy.store((level * 255.0) as u8, Ordering::Relaxed);
                let _ = self.tx.send(s.to_vec());
            }
        }
        if let Err(e) = cap.start(Cb { tx, quit: audio_quit, energy: audio_energy }) {
            log::error!("音频采集失败: {}", e);
        }
        loop { thread::sleep(Duration::from_secs(3600)); }
    });

    // ═══ 6. 全部就绪 → UI 切换到"就绪"状态 ═══
    ui_progress.store(100, Ordering::Relaxed);
    thread::sleep(Duration::from_millis(300)); // 让用户看到 100%
    ui_phase.store(1, Ordering::Relaxed); // Ready
    log::info!("✅ 全部就绪，等待语音输入");

    // ═══ 7. 主循环 ═══
    loop {
        if quit.load(Ordering::Relaxed) { break; }

        while let Ok(samples) = rx.try_recv() {
            if samples.is_empty() { continue; }
            vad.feed(&samples);

            // 根据 VAD 状态切换 UI 阶段
            match vad.state() {
                vad::VadState::SpeechStarted | vad::VadState::Speaking => {
                    ui_phase.store(2, Ordering::Relaxed); // Listening
                }
                vad::VadState::SpeechEnded => {
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
                                    log::error!("输出: {}", e);
                                }
                            }
                            Ok(_) => {}
                            Err(e) => log::error!("识别: {}", e),
                        }
                    }
                    vad.reset();
                    ui_phase.store(1, Ordering::Relaxed); // 回到 Ready
                }
                vad::VadState::Silence => {
                    ui_phase.store(1, Ordering::Relaxed); // Ready
                }
            }
        }

        thread::sleep(Duration::from_millis(10));
    }

    // 退出
    learner.flush();
    let _ = ui_handle.join();
}