// src/main.rs — Voice IME v0.5.0
// 按住热键录音 → 流式解码 → 纠错+标点 → 预览 → 自动输出

mod config;
mod audio;
mod asr;
mod output;
mod ui;
mod commands;
mod hotkey;
mod settings;
mod bigram;
mod correct;
mod punctuation;
mod preview;

use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::thread;

use crate::commands::{MatchResult, Action};
use crate::asr::StreamHandle;

/// 应用状态机
enum AppState {
    Idle,
    Recording { handle: StreamHandle, start: Instant },
    Previewing { text: String, start: Instant },
}

/// 应用核心
struct App {
    asr: asr::AsrEngine,
    corrector: correct::CorrectionEngine,
    cmd_engine: commands::CommandEngine,
    keyboard: output::KeyboardOutput,
    preview: preview::PreviewWindow,
    // UI 共享状态
    ui_phase: Arc<AtomicU8>,
    ui_energy: Arc<AtomicU8>,
    ui_flash: Arc<AtomicU8>,
    ui_open_settings: Arc<AtomicBool>,
    // 通道
    audio_rx: mpsc::Receiver<Vec<f32>>,
    hk_rx: mpsc::Receiver<hotkey::HotkeyEvent>,
    // 控制
    quit: Arc<AtomicBool>,
    config: config::AppConfig,
}

impl App {
    fn run(&mut self) {
        let mut state = AppState::Idle;

        loop {
            if self.quit.load(Ordering::Relaxed) { break; }

            // 设置页面请求
            if self.ui_open_settings.swap(false, Ordering::Relaxed) {
                settings::open_settings(&self.config.model_dir());
            }

            // 热键事件
            while let Ok(ev) = self.hk_rx.try_recv() {
                match ev {
                    hotkey::HotkeyEvent::KeyDown => {
                        let handle = self.asr.new_stream();
                        self.ui_phase.store(2, Ordering::Relaxed);
                        state = AppState::Recording { handle, start: Instant::now() };
                    }
                    hotkey::HotkeyEvent::KeyUp => {
                        if let AppState::Recording { mut handle, start } = state {
                            // 消费剩余音频
                            while let Ok(samples) = self.audio_rx.try_recv() {
                                if !samples.is_empty() {
                                    self.asr.feed_and_decode(&mut handle, &samples);
                                }
                            }
                            thread::sleep(Duration::from_millis(100));
                            while let Ok(samples) = self.audio_rx.try_recv() {
                                if !samples.is_empty() {
                                    self.asr.feed_and_decode(&mut handle, &samples);
                                }
                            }

                            let raw = self.asr.finish_stream(&mut handle);
                            let dur = start.elapsed().as_millis();

                            if !raw.is_empty() {
                                // 命令匹配
                                if let MatchResult::Matched(action) = self.cmd_engine.match_text(&raw) {
                                    log::info!("🎯 命令: {} → {:?}", raw, action);
                                    self.ui_flash.store(15, Ordering::Relaxed);
                                    self.execute_action(action);
                                    state = AppState::Idle;
                                    self.ui_phase.store(1, Ordering::Relaxed);
                                    continue;
                                }

                                // 纠错 + 标点
                                let corrected = self.corrector.correct(&raw);
                                let final_text = if self.asr.is_sense_voice() {
                                    corrected // SenseVoice 自带标点
                                } else {
                                    punctuation::add_punctuation(&corrected)
                                };

                                log::info!("🎤 {} ({}ms)", final_text, dur);
                                self.preview.show_text(&final_text);
                                state = AppState::Previewing {
                                    text: final_text,
                                    start: Instant::now(),
                                };
                            } else {
                                state = AppState::Idle;
                            }
                            self.ui_phase.store(1, Ordering::Relaxed);
                        } else {
                            state = AppState::Idle;
                        }
                    }
                }
            }

            // 录音中：喂音频 + 实时预览
            if let AppState::Recording { ref mut handle, .. } = state {
                while let Ok(samples) = self.audio_rx.try_recv() {
                    if !samples.is_empty() {
                        self.asr.feed_and_decode(handle, &samples);
                    }
                }
                let partial = self.asr.partial_result(handle);
                if !partial.is_empty() {
                    self.preview.update_text(&partial);
                }
            }

            // 预览 → 1秒后自动输出
            if let AppState::Previewing { ref text, ref start } = state {
                if start.elapsed() >= Duration::from_secs(1) {
                    if !text.is_empty() {
                        if let Err(e) = self.keyboard.send_text(text) {
                            log::error!("输出: {}", e);
                        }
                        self.corrector.record(text);
                        log::info!("📤 已输出");
                    }
                    self.preview.hide();
                    state = AppState::Idle;
                }
            }

            // 空闲：丢弃音频
            if let AppState::Idle = state {
                while self.audio_rx.try_recv().is_ok() {}
                self.ui_energy.store(0, Ordering::Relaxed);
            }

            // 用 recv_timeout 替代 sleep（更省电）
            thread::sleep(Duration::from_millis(15));
        }

        self.corrector.flush();
    }

    fn execute_action(&mut self, action: Action) {
        match action {
            Action::Launch(t) => { commands::exec_launch(&t).ok(); }
            Action::OpenUrl(u) => { commands::exec_open_url(&u).ok(); }
            Action::Hotkey(steps) => { commands::exec_hotkey(&steps).ok(); }
            Action::Text(t) => { self.keyboard.send_text(&t).ok(); }
        }
    }
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
    let ui_handle = thread::spawn(move || ui::run_ui(ui_state));

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

    // ═══ 3. ASR ═══
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

    // ═══ 4. 纠错 + 命令 ═══
    ui_progress.store(80, Ordering::Relaxed);
    let corrector = correct::CorrectionEngine::new(&config.model_dir());
    let cmd_engine = commands::CommandEngine::load(&config.commands_file());
    let keyboard = output::KeyboardOutput::new();
    let preview = preview::PreviewWindow::new();

    // ═══ 5. 热键 ═══
    ui_progress.store(90, Ordering::Relaxed);
    let (hk_tx, hk_rx) = mpsc::channel();
    if let Err(e) = hotkey::spawn_hotkey_thread(&config.hotkey.toggle, hk_tx) {
        hotkey::spawn_warning_dialog_async(format!("热键失败: {}\n修改 voice_ime.toml [hotkey]", e));
    }

    // ═══ 6. 音频 ═══
    ui_progress.store(95, Ordering::Relaxed);
    let (audio_tx, audio_rx) = mpsc::channel();
    let acfg = config.audio.clone();
    let a_energy = ui_energy.clone();
    let a_quit = quit.clone();

    thread::spawn(move || {
        let cap = audio::AudioCapture::new(acfg);
        struct Cb {
            tx: mpsc::Sender<Vec<f32>>,
            quit: Arc<AtomicBool>,
            energy: Arc<AtomicU8>,
        }
        impl audio::AudioCallback for Cb {
            fn on_audio_data(&mut self, s: &[f32], _: u32) {
                if self.quit.load(Ordering::Relaxed) { return; }
                let rms = audio::compute_energy(s);
                self.energy.store(((rms * 10.0).clamp(0.0, 1.0) * 255.0) as u8, Ordering::Relaxed);
                let _ = self.tx.send(s.to_vec());
            }
        }
        if let Err(e) = cap.start(Cb { tx: audio_tx, quit: a_quit, energy: a_energy }) {
            log::error!("音频: {}", e);
        }
        loop { thread::sleep(Duration::from_secs(3600)); }
    });

    // ═══ 7. 就绪 ═══
    ui_progress.store(100, Ordering::Relaxed);
    thread::sleep(Duration::from_millis(300));
    ui_phase.store(1, Ordering::Relaxed);
    log::info!("✅ 就绪 | 按住 {} 说话 | 模型: {}",
        config.hotkey.toggle,
        if config.is_sense_voice() { "SenseVoice" } else { "Transducer" });

    // ═══ 8. 运行 ═══
    let mut app = App {
        asr, corrector, cmd_engine, keyboard, preview,
        ui_phase, ui_energy, ui_flash, ui_open_settings,
        audio_rx, hk_rx, quit: quit.clone(), config,
    };
    app.run();
    let _ = ui_handle.join();
}