// src/main.rs — Voice IME 入口
// 启动: UI窗口(进度) → 模型初始化 → 就绪 → 语音识别/命令执行

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

use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::thread;

use crate::commands::{Action, BuiltinCmd, MatchResult};

fn main() {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Info,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    ).ok();

    // ═══ 1. UI 窗口（加载进度） ═══
    let quit = Arc::new(AtomicBool::new(false));
    let listening = Arc::new(AtomicBool::new(true));
    let ui_state = ui::UiState::new(quit.clone());
    let ui_energy = ui_state.energy.clone();
    let ui_phase = ui_state.phase.clone();
    let ui_progress = ui_state.progress.clone();
    let ui_flash = ui_state.flash_frames.clone();
    let ui_open_settings = ui_state.open_settings.clone();

    ui_state.set_phase_loading();
    ui_state.set_progress(0);

    let ui_handle = thread::spawn(move || {
        ui::run_ui(ui_state);
    });

    // ═══ 2. 配置 ═══
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

    // ═══ 3. ASR 模型 ═══
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

    // ═══ 4. 其他模块 ═══
    ui_progress.store(85, Ordering::Relaxed);
    let learner = learn::LearningEngine::new(&config.model_dir());
    let cmd_engine = commands::CommandEngine::load(&config.commands_file());
    log::info!("命令引擎就绪: {} 条命令", cmd_engine.len());

    ui_progress.store(90, Ordering::Relaxed);
    let mut keyboard = output::KeyboardOutput::new();
    let mut vad = vad::VoiceDetect::new(&config.audio);

    // ═══ 5. 全局热键 ═══
    ui_progress.store(93, Ordering::Relaxed);
    let (hk_tx, hk_rx) = mpsc::channel::<hotkey::HotkeyEvent>();
    match hotkey::spawn_hotkey_thread(&config.hotkey.toggle, hk_tx) {
        Ok(()) => log::info!("全局热键注册成功: {}", config.hotkey.toggle),
        Err(e) => {
            log::warn!("热键注册失败: {}", e);
            hotkey::spawn_warning_dialog_async(format!(
                "{}\n\n请修改 voice_ime.toml 中 [hotkey] toggle 后重启程序。\n\n当前配置: {}",
                e, config.hotkey.toggle
            ));
        }
    }

    // ═══ 6. 音频采集 ═══
    ui_progress.store(96, Ordering::Relaxed);
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

    // ═══ 7. 全部就绪 ═══
    ui_progress.store(100, Ordering::Relaxed);
    thread::sleep(Duration::from_millis(300));
    ui_phase.store(1, Ordering::Relaxed); // Ready
    log::info!("✅ 全部就绪 | 右键窗口或说话开始");

    // ═══ 8. 主循环 ═══
    loop {
        if quit.load(Ordering::Relaxed) { break; }

        // 设置页面请求
        if ui_open_settings.swap(false, Ordering::Relaxed) {
            settings::open_settings(&config.model_dir());
        }

        // 热键事件
        while let Ok(ev) = hk_rx.try_recv() {
            match ev {
                hotkey::HotkeyEvent::Toggle => {
                    let cur = listening.load(Ordering::Relaxed);
                    let new_state = !cur;
                    listening.store(new_state, Ordering::Relaxed);
                    if new_state {
                        ui_phase.store(1, Ordering::Relaxed);
                        log::info!("▶ 恢复监听 (热键)");
                    } else {
                        ui_phase.store(3, Ordering::Relaxed);
                        vad.reset();
                        ui_energy.store(0, Ordering::Relaxed);
                        log::info!("⏸ 暂停监听 (热键)");
                    }
                    ui_flash.store(15, Ordering::Relaxed);
                }
            }
        }

        // 音频处理
        while let Ok(samples) = rx.try_recv() {
            if samples.is_empty() { continue; }

            // 暂停时完全丢弃
            if !listening.load(Ordering::Relaxed) {
                if vad.state() != vad::VadState::Silence { vad.reset(); }
                ui_energy.store(0, Ordering::Relaxed);
                continue;
            }

            vad.feed(&samples);

            match vad.state() {
                vad::VadState::SpeechStarted | vad::VadState::Speaking => {
                    ui_phase.store(2, Ordering::Relaxed);
                }
                vad::VadState::SpeechEnded => {
                    let speech = vad.get_speech().to_vec();
                    if !speech.is_empty() {
                        match asr.recognize(&speech) {
                            Ok(r) if !r.text.is_empty() => {
                                // 先匹配命令
                                match cmd_engine.match_text(&r.text) {
                                    MatchResult::Matched(action) => {
                                        log::info!("🎯 命令触发: {} → {:?}", r.text, action);
                                        ui_flash.store(15, Ordering::Relaxed);
                                        execute_action(
                                            action,
                                            &listening,
                                            &ui_phase,
                                            &ui_energy,
                                            &mut keyboard,
                                            &mut vad,
                                        );
                                    }
                                    MatchResult::NoMatch => {
                                        // 正常流程: 纠错 + 输出
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
                                }
                            }
                            Ok(_) => {}
                            Err(e) => log::error!("识别: {}", e),
                        }
                    }
                    vad.reset();
                    // 根据 listening 恢复到正确状态
                    if listening.load(Ordering::Relaxed) {
                        ui_phase.store(1, Ordering::Relaxed);
                    } else {
                        ui_phase.store(3, Ordering::Relaxed);
                    }
                }
                vad::VadState::Silence => {
                    if listening.load(Ordering::Relaxed) {
                        ui_phase.store(1, Ordering::Relaxed);
                    }
                }
            }
        }

        thread::sleep(Duration::from_millis(10));
    }

    learner.flush();
    let _ = ui_handle.join();
}

/// 执行命令动作
fn execute_action(
    action: Action,
    listening: &Arc<AtomicBool>,
    ui_phase: &Arc<std::sync::atomic::AtomicU8>,
    ui_energy: &Arc<std::sync::atomic::AtomicU8>,
    keyboard: &mut output::KeyboardOutput,
    vad: &mut vad::VoiceDetect,
) {
    match action {
        Action::Launch(target) => {
            if let Err(e) = commands::exec_launch(&target) {
                log::error!("{}", e);
            }
        }
        Action::OpenUrl(url) => {
            if let Err(e) = commands::exec_open_url(&url) {
                log::error!("{}", e);
            }
        }
        Action::Hotkey(steps) => {
            if let Err(e) = commands::exec_hotkey(&steps) {
                log::error!("{}", e);
            }
        }
        Action::Text(text) => {
            if let Err(e) = keyboard.send_text(&text) {
                log::error!("文本输出失败: {}", e);
            }
        }
        Action::Builtin(cmd) => {
            let cur = listening.load(Ordering::Relaxed);
            let new_state = match cmd {
                BuiltinCmd::Pause => false,
                BuiltinCmd::Resume => true,
                BuiltinCmd::Toggle => !cur,
            };
            listening.store(new_state, Ordering::Relaxed);
            if new_state {
                ui_phase.store(1, Ordering::Relaxed);
                log::info!("▶ 恢复监听 (语音命令)");
            } else {
                ui_phase.store(3, Ordering::Relaxed);
                vad.reset();
                ui_energy.store(0, Ordering::Relaxed);
                log::info!("⏸ 暂停监听 (语音命令)");
            }
        }
    }
}