// src/main.rs — Voice IME v0.4.0
// 按住热键录音 → 流式解码 → 纠错+标点 → 预览 → 自动输出

mod config;
mod audio;
mod vad;
mod asr;
mod output;
mod learn;
mod ui;
mod commands;
mod hotkey;
mod settings;
mod bigram;
mod correct;
mod punctuation;
mod preview;

use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::thread;

use crate::commands::{MatchResult, Action, BuiltinCmd};

/// 应用状态机
enum AppState {
    Idle,
    Recording { stream: sherpa_onnx::OnlineStream, start: Instant },
    Previewing { text: String, start: Instant },
}

fn main() {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Info,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    ).ok();

    // ═══ 1. UI 窗口 ═══
    let quit = Arc::new(AtomicBool::new(false));
    let ui_state = ui::UiState::new(quit.clone());
    let ui_energy = ui_state.energy.clone();
    let ui_phase = ui_state.phase.clone();
    let ui_progress = ui_state.progress.clone();
    let ui_flash = ui_state.flash_frames.clone();
    let ui_open_settings = ui_state.open_settings.clone();

    ui_state.set_phase_loading();
    ui_state.set_progress(0);

    let ui_handle = thread::spawn(move || { ui::run_ui(ui_state); });

    // ═══ 2. 配置 ═══
    ui_progress.store(10, Ordering::Relaxed);
    let config = config::AppConfig::load_or_default();
    if let Err(e) = config.validate() {
        log::error!("{}", e);
        quit.store(true, Ordering::Relaxed);
        let _ = ui_handle.join();
        std::process::exit(1);
    }
    ui_progress.store(20, Ordering::Relaxed);

    // ═══ 3. ASR 模型 ═══
    ui_progress.store(30, Ordering::Relaxed);
    let asr = match asr::AsrEngine::new(&config) {
        Ok(e) => { ui_progress.store(70, Ordering::Relaxed); e }
        Err(e) => {
            log::error!("ASR: {}", e);
            quit.store(true, Ordering::Relaxed);
            let _ = ui_handle.join();
            std::process::exit(1);
        }
    };

    // ═══ 4. 纠错引擎 + 命令 ═══
    ui_progress.store(80, Ordering::Relaxed);
    let mut corrector = correct::CorrectionEngine::new(&config.model_dir());
    let cmd_engine = commands::CommandEngine::load(&config.commands_file());
    let mut keyboard = output::KeyboardOutput::new();
    let preview = preview::PreviewWindow::new();

    // ═══ 5. 热键 ═══
    ui_progress.store(90, Ordering::Relaxed);
    let (hk_tx, hk_rx) = mpsc::channel::<hotkey::HotkeyEvent>();
    if let Err(e) = hotkey::spawn_hotkey_thread(&config.hotkey.toggle, hk_tx) {
        hotkey::spawn_warning_dialog_async(format!("热键失败: {}\n修改 voice_ime.toml [hotkey]", e));
    }

    // ═══ 6. 音频采集 ═══
    ui_progress.store(95, Ordering::Relaxed);
    let (audio_tx, audio_rx) = mpsc::channel::<Vec<f32>>();
    let acfg = config.audio.clone();
    let audio_energy = ui_energy.clone();
    let audio_quit = quit.clone();

    thread::spawn(move || {
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
        if let Err(e) = cap.start(Cb { tx: audio_tx, quit: audio_quit, energy: audio_energy }) {
            log::error!("音频: {}", e);
        }
        loop { thread::sleep(Duration::from_secs(3600)); }
    });

    // ═══ 7. 就绪 ═══
    ui_progress.store(100, Ordering::Relaxed);
    thread::sleep(Duration::from_millis(300));
    ui_phase.store(1, Ordering::Relaxed);
    log::info!("✅ 就绪 | 按住 {} 说话", config.hotkey.toggle);

    // ═══ 8. 主循环 ═══
    let mut state = AppState::Idle;

    loop {
        if quit.load(Ordering::Relaxed) { break; }

        // 设置页面
        if ui_open_settings.swap(false, Ordering::Relaxed) {
            settings::open_settings(&config.model_dir());
        }

        // 热键事件
        while let Ok(ev) = hk_rx.try_recv() {
            match ev {
                hotkey::HotkeyEvent::KeyDown => {
                    // 开始录音
                    let stream = asr.new_stream();
                    ui_phase.store(2, Ordering::Relaxed); // 波纹
                    log::debug!("🎙 录音开始");
                    state = AppState::Recording { stream, start: Instant::now() };
                }
                hotkey::HotkeyEvent::KeyUp => {
                    if let AppState::Recording { stream, start } = &state {
                        // 停止录音 → 获取最终结果
                        let raw = asr.finish_stream(stream);
                        let dur = start.elapsed().as_millis();

                        if !raw.is_empty() {
                            // 纠错 + 标点
                            let corrected = corrector.correct(&raw);
                            let final_text = punctuation::add_punctuation(&corrected);

                            log::info!("🎤 {} ({}ms)", final_text, dur);

                            // 先检查是否为命令
                            match cmd_engine.match_text(&raw) {
                                MatchResult::Matched(action) => {
                                    log::info!("🎯 命令: {:?}", action);
                                    ui_flash.store(15, Ordering::Relaxed);
                                    execute_action(action, &mut keyboard);
                                    state = AppState::Idle;
                                    ui_phase.store(1, Ordering::Relaxed);
                                    continue;
                                }
                                MatchResult::NoMatch => {}
                            }

                            // 预览
                            preview.show_text(&final_text);
                            state = AppState::Previewing {
                                text: final_text,
                                start: Instant::now(),
                            };
                        } else {
                            state = AppState::Idle;
                        }
                        ui_phase.store(1, Ordering::Relaxed);
                    }
                }
            }
        }

        // 录音中：喂音频 + 实时预览
        if let AppState::Recording { ref stream, .. } = state {
            while let Ok(samples) = audio_rx.try_recv() {
                if !samples.is_empty() {
                    asr.feed_and_decode(stream, &samples);
                }
            }
            // 实时显示中间结果
            let partial = asr.partial_result(stream);
            if !partial.is_empty() {
                preview.update_text(&partial);
            }
        }

        // 预览 → 1秒后自动输出
        if let AppState::Previewing { ref text, ref start } = state {
            if start.elapsed() >= Duration::from_secs(1) {
                if !text.is_empty() {
                    if let Err(e) = keyboard.send_text(text) {
                        log::error!("输出: {}", e);
                    }
                    // 更新 bigram 频率
                    corrector.record(text);
                    log::info!("📤 已输出");
                }
                preview.hide();
                state = AppState::Idle;
            }
        }

        // 空闲：丢弃音频
        if let AppState::Idle = state {
            while audio_rx.try_recv().is_ok() {}
            ui_energy.store(0, Ordering::Relaxed);
        }

        thread::sleep(Duration::from_millis(10));
    }

    corrector.flush();
    let _ = ui_handle.join();
}

fn execute_action(action: Action, keyboard: &mut output::KeyboardOutput) {
    match action {
        Action::Launch(t) => { commands::exec_launch(&t).ok(); }
        Action::OpenUrl(u) => { commands::exec_open_url(&u).ok(); }
        Action::Hotkey(steps) => { commands::exec_hotkey(&steps).ok(); }
        Action::Text(t) => { keyboard.send_text(&t).ok(); }
        Action::Builtin(_) => { /* 热键模式下不需要 pause/resume */ }
    }
}