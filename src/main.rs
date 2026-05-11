// src/main.rs — Voice IME 入口

mod config;
mod audio;
mod vad;
mod asr;
mod output;
mod learn;

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
        println!("请将以下文件放入 models/ 目录:");
        println!("  encoder-epoch-*.onnx  decoder-epoch-*.onnx");
        println!("  joiner-epoch-*.onnx   tokens.txt");
        println!("\n下载地址: https://github.com/k2-fsa/sherpa-onnx/releases");
        std::process::exit(1);
    }
    log::info!("模型目录: {}", config.model_dir_str());

    // ASR
    let asr = match asr::AsrEngine::new(&config) {
        Ok(e) => { log::info!("ASR 就绪"); e }
        Err(e) => { eprintln!("❌ ASR 失败: {}", e); std::process::exit(1); }
    };

    // 自学习引擎
    let learner = learn::LearningEngine::new(&config.model_dir());
    let (vocab_count, total_freq) = learner.stats();
    if vocab_count > 0 {
        log::info!("已加载历史词频: {} 词, 共 {} 次", vocab_count, total_freq);
    }

    // 输出
    let mut keyboard = output::KeyboardOutput::new();

    // VAD
    let mut vad = vad::VoiceDetect::new(&config.audio);

    // 音频采集
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    let (tx, rx) = mpsc::channel::<Vec<f32>>();
    let acfg = config.audio.clone();

    let _h = thread::spawn(move || {
        let cap = audio::AudioCapture::new(acfg);
        struct Cb { tx: mpsc::Sender<Vec<f32>>, r: Arc<AtomicBool> }
        impl audio::AudioCallback for Cb {
            fn on_audio_data(&mut self, s: &[f32], _: u32) {
                if self.r.load(Ordering::Relaxed) { let _ = self.tx.send(s.to_vec()); }
            }
        }
        if let Err(e) = cap.start(Cb { tx, r }) {
            log::error!("音频采集失败: {}", e);
        }
        loop { thread::sleep(Duration::from_secs(3600)); }
    });

    println!("\n  Enter = 开始/停止  |  Q = 退出");
    println!("  识别结果会自动纠错并记录词频\n");
    println!("✅ 就绪\n");
    let mut listening = true;

    loop {
        while let Ok(samples) = rx.try_recv() {
            if !listening || samples.is_empty() { continue; }
            vad.feed(&samples);

            if vad.state() == vad::VadState::SpeechEnded {
                let speech = vad.get_speech().to_vec();
                if !speech.is_empty() {
                    print!("\r🔍 识别中...        ");
                    io::stdout().flush().ok();

                    match asr.recognize(&speech) {
                        Ok(r) if !r.text.is_empty() => {
                            // 自学习: 纠错 + 记录词频
                            let final_text = learner.process(&r.text);

                            if final_text != r.text {
                                println!("\r🎤 {} → {} ({}ms)",
                                    r.text, final_text, r.latency_ms);
                            } else {
                                println!("\r🎤 {} ({}ms)", final_text, r.latency_ms);
                            }

                            if let Err(e) = keyboard.send_text(&final_text) {
                                eprintln!("⚠ 输出: {}", e);
                            }
                        }
                        Ok(_) => { print!("\r                    \r"); }
                        Err(e) => { eprintln!("\r⚠ {}", e); }
                    }
                }
                vad.reset();
            }
        }

        // 按键
        if let Some(c) = read_key() {
            match c {
                'q' => {
                    // 退出前保存词频
                    learner.flush();
                    let (vc, tf) = learner.stats();
                    println!("📊 本次会话统计: {} 个词汇, 共 {} 次识别", vc, tf);
                    println!("退出...");
                    break;
                }
                _ => {
                    listening = !listening;
                    println!("{}", if listening { "[▶] 监听中" } else { "[⏸] 已停止" });
                }
            }
        }
        thread::sleep(Duration::from_millis(10));
    }

    running.store(false, Ordering::Relaxed);
}

#[cfg(target_os = "windows")]
fn read_key() -> Option<char> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    unsafe {
        if GetAsyncKeyState(0x0D) < 0 { thread::sleep(Duration::from_millis(250)); return Some('\n'); }
        if GetAsyncKeyState(0x51) < 0 { thread::sleep(Duration::from_millis(250)); return Some('q'); }
    }
    None
}

#[cfg(not(target_os = "windows"))]
fn read_key() -> Option<char> { None }