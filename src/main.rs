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
    Previewing { text: String, original_raw: String, start: Instant },
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
                                    original_raw: raw,
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
                    hotkey::HotkeyEvent::EscPressed => {
                        // 预览状态下按 Esc → 取消输出，弹出纠错对话框
                        if let AppState::Previewing { text, original_raw, .. } = &state {
                            self.preview.hide();
                            let prefill = text.clone();

                            if let Some(corrected) = show_correction_dialog(&prefill) {
                                if corrected != prefill && !corrected.is_empty() {
                                    // 自动学习
                                    self.corrector.learn_from_correction(&prefill, &corrected);
                                }
                                // 输出修改后的文本
                                if !corrected.is_empty() {
                                    if let Err(e) = self.keyboard.send_text(&corrected) {
                                        log::error!("输出: {}", e);
                                    }
                                    self.corrector.record(&corrected);
                                    log::info!("📤 已输出(纠正): {}", corrected);
                                }
                            }
                            state = AppState::Idle;
                            self.ui_phase.store(1, Ordering::Relaxed);
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
            if let AppState::Previewing { ref text, ref start, .. } = state {
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

/// Win32 弹出纠错对话框（预填文字，用户修改后返回）
/// 返回 Some(修改后文字) 或 None（取消）
#[cfg(target_os = "windows")]
fn show_correction_dialog(prefill: &str) -> Option<String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

    use std::sync::{Arc, Mutex};

    // 用静态变量存储 edit 控件句柄和结果（WndProc 需要访问）
    static mut HEDIT: isize = 0;
    static mut DLG_RESULT: i32 = 0; // 0=pending, 1=ok, 2=cancel

    unsafe extern "system" fn dlg_wndproc(
        hwnd: isize, msg: u32, wparam: usize, lparam: isize,
    ) -> isize {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;

        match msg {
            WM_COMMAND => {
                let id = (wparam & 0xFFFF) as i32;
                let notify = ((wparam >> 16) & 0xFFFF) as i32;
                // BN_CLICKED = 0
                if id == 1 && notify == 0 { // 确认按钮
                    DLG_RESULT = 1;
                    PostQuitMessage(0);
                    return 0;
                }
                if id == 2 && notify == 0 { // 取消按钮
                    DLG_RESULT = 2;
                    PostQuitMessage(0);
                    return 0;
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_KEYDOWN => {
                if wparam == 0x0D { // VK_RETURN
                    DLG_RESULT = 1;
                    PostQuitMessage(0);
                    return 0;
                }
                if wparam == 0x1B { // VK_ESCAPE
                    DLG_RESULT = 2;
                    PostQuitMessage(0);
                    return 0;
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_CLOSE => {
                DLG_RESULT = 2;
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    let result: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let result_clone = result.clone();
    let prefill_owned = prefill.to_string();

    let handle = std::thread::spawn(move || {
        unsafe {
            DLG_RESULT = 0;
            HEDIT = 0;

            let hinstance = GetModuleHandleW(std::ptr::null());
            let class_name: Vec<u16> = "VoiceIME_CorrDlg\0".encode_utf16().collect();

            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(dlg_wndproc),
                cbClsExtra: 0, cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: 0, hCursor: LoadCursorW(0, IDC_ARROW),
                hbrBackground: (COLOR_BTNFACE + 1) as isize,
                lpszMenuName: std::ptr::null(),
                lpszClassName: class_name.as_ptr(),
            };
            RegisterClassW(&wc);

            let screen_w = GetSystemMetrics(SM_CXSCREEN);
            let dlg_w = 420;
            let dlg_h = 110;
            let x = (screen_w - dlg_w) / 2;
            let y = 60;

            let title: Vec<u16> = "纠正识别结果 (Enter=确认, Esc=取消)\0".encode_utf16().collect();
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                class_name.as_ptr(),
                title.as_ptr(),
                WS_POPUP | WS_VISIBLE | WS_CAPTION | WS_SYSMENU,
                x, y, dlg_w, dlg_h,
                0, 0, hinstance, std::ptr::null(),
            );
            if hwnd == 0 { return; }

            // Edit 控件
            let edit_class: Vec<u16> = "EDIT\0".encode_utf16().collect();
            let edit_text: Vec<u16> = prefill_owned.encode_utf16().chain(Some(0)).collect();
            let hedit = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                edit_class.as_ptr(),
                edit_text.as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32,
                10, 10, dlg_w - 20, 26,
                hwnd, 1001 as isize, hinstance, std::ptr::null(),
            );
            HEDIT = hedit;

            // 字体
            let font_name: Vec<u16> = "Microsoft YaHei\0".encode_utf16().collect();
            let hfont = CreateFontW(
                -15, 0, 0, 0, FW_NORMAL as i32, 0, 0, 0,
                DEFAULT_CHARSET as u32, OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32, CLEARTYPE_QUALITY as u32,
                DEFAULT_PITCH as u32, font_name.as_ptr(),
            );
            SendMessageW(hedit, WM_SETFONT, hfont as usize, 1);
            SendMessageW(hedit, 0x00B1, 0, -1_isize); // EM_SETSEL 全选
            SetFocus(hedit);

            // 按钮
            let btn_class: Vec<u16> = "BUTTON\0".encode_utf16().collect();
            let btn_ok: Vec<u16> = "确认(Enter)\0".encode_utf16().collect();
            let btn_cancel: Vec<u16> = "取消(Esc)\0".encode_utf16().collect();

            let btn_ok_h = CreateWindowExW(
                0, btn_class.as_ptr(), btn_ok.as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
                dlg_w - 220, 48, 100, 30,
                hwnd, 1 as isize, hinstance, std::ptr::null(),
            );
            let btn_cancel_h = CreateWindowExW(
                0, btn_class.as_ptr(), btn_cancel.as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as u32,
                dlg_w - 110, 48, 100, 30,
                hwnd, 2 as isize, hinstance, std::ptr::null(),
            );
            // 按钮也设字体
            SendMessageW(btn_ok_h, WM_SETFONT, hfont as usize, 1);
            SendMessageW(btn_cancel_h, WM_SETFONT, hfont as usize, 1);

            // 消息循环
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                // 让 Tab 键在控件间切换
                if IsDialogMessageW(hwnd, &msg) == 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }

            // 读取结果
            if DLG_RESULT == 1 {
                let mut buf = vec![0u16; 2048];
                let len = GetWindowTextW(hedit, buf.as_mut_ptr(), buf.len() as i32);
                let text = String::from_utf16_lossy(&buf[..len as usize]);
                *result_clone.lock().unwrap() = Some(text);
            }

            DestroyWindow(hwnd);
            DeleteObject(hfont as isize);
            UnregisterClassW(class_name.as_ptr(), hinstance);
        }
    });

    let _ = handle.join();
    let guard = result.lock().unwrap();
    guard.clone()
}

#[cfg(not(target_os = "windows"))]
fn show_correction_dialog(_prefill: &str) -> Option<String> {
    None
}