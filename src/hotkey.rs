// src/hotkey.rs — Win32 全局热键
//
// 用法:
//   let (tx, rx) = mpsc::channel();
//   hotkey::spawn_hotkey_thread("Ctrl+Alt+V", tx)?;
//   // 主循环中 rx.try_recv() 接收 HotkeyEvent

use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy)]
pub enum HotkeyEvent {
    Toggle,
}

/// 解析热键字符串 "Ctrl+Alt+V" → (modifiers_mask, vk_code)
fn parse_hotkey(s: &str) -> Result<(u32, u32), String> {
    // Windows 常量
    const MOD_ALT: u32 = 0x0001;
    const MOD_CONTROL: u32 = 0x0002;
    const MOD_SHIFT: u32 = 0x0004;
    const MOD_WIN: u32 = 0x0008;

    let mut modifiers: u32 = 0;
    let mut vk: Option<u32> = None;

    for part in s.split('+') {
        let p = part.trim().to_lowercase();
        match p.as_str() {
            "ctrl" | "control" => modifiers |= MOD_CONTROL,
            "alt" => modifiers |= MOD_ALT,
            "shift" => modifiers |= MOD_SHIFT,
            "win" | "meta" | "super" | "cmd" => modifiers |= MOD_WIN,
            "" => {}
            other => {
                // 主键
                if vk.is_some() {
                    return Err(format!("多个主键: {}", s));
                }
                vk = Some(parse_vk(other)?);
            }
        }
    }

    match vk {
        Some(k) => Ok((modifiers, k)),
        None => Err(format!("缺少主键: {}", s)),
    }
}

/// 虚拟键码映射
fn parse_vk(s: &str) -> Result<u32, String> {
    let lower = s.to_lowercase();
    // 字母 A-Z: 0x41..0x5A
    if lower.chars().count() == 1 {
        let ch = lower.chars().next().unwrap();
        if ch.is_ascii_alphabetic() {
            return Ok(0x41 + (ch.to_ascii_uppercase() as u32 - 'A' as u32));
        }
        if ch.is_ascii_digit() {
            return Ok(0x30 + (ch as u32 - '0' as u32));
        }
    }
    match lower.as_str() {
        "f1" => Ok(0x70),  "f2" => Ok(0x71),  "f3" => Ok(0x72),  "f4" => Ok(0x73),
        "f5" => Ok(0x74),  "f6" => Ok(0x75),  "f7" => Ok(0x76),  "f8" => Ok(0x77),
        "f9" => Ok(0x78),  "f10" => Ok(0x79), "f11" => Ok(0x7A), "f12" => Ok(0x7B),
        "space" => Ok(0x20),
        "enter" | "return" => Ok(0x0D),
        "tab" => Ok(0x09),
        "escape" | "esc" => Ok(0x1B),
        "backspace" => Ok(0x08),
        "delete" | "del" => Ok(0x2E),
        "home" => Ok(0x24),
        "end" => Ok(0x23),
        "pageup" => Ok(0x21),
        "pagedown" => Ok(0x22),
        "up" => Ok(0x26),
        "down" => Ok(0x28),
        "left" => Ok(0x25),
        "right" => Ok(0x27),
        _ => Err(format!("未知按键: {}", s)),
    }
}

/// 启动热键监听线程
/// 成功返回 Ok(())，失败返回错误信息（调用方可弹窗提示）
pub fn spawn_hotkey_thread(
    hotkey: &str,
    tx: Sender<HotkeyEvent>,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let (modifiers, vk) = parse_hotkey(hotkey)?;
        let hotkey_str = hotkey.to_string();

        // 为了验证热键能否注册，先在本线程试一次
        // 但 RegisterHotKey 注册在哪个线程就只能在哪个线程接收 WM_HOTKEY
        // 所以必须在独立线程里 register + GetMessage
        let (init_tx, init_rx) = std::sync::mpsc::channel::<Result<(), String>>();

        std::thread::spawn(move || {
            use windows_sys::Win32::UI::WindowsAndMessaging::*;
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};

            unsafe {
                let ok = RegisterHotKey(0, 1, modifiers, vk);
                if ok == 0 {
                    let _ = init_tx.send(Err(format!(
                        "热键 '{}' 已被其他程序占用", hotkey_str
                    )));
                    return;
                }
                let _ = init_tx.send(Ok(()));

                let mut msg: MSG = std::mem::zeroed();
                loop {
                    let ret = GetMessageW(&mut msg, 0, 0, 0);
                    if ret <= 0 { break; }
                    if msg.message == WM_HOTKEY && msg.wParam == 1 {
                        if tx.send(HotkeyEvent::Toggle).is_err() {
                            break;
                        }
                    }
                }

                UnregisterHotKey(0, 1);
            }
        });

        // 等待注册结果
        init_rx.recv().map_err(|_| "热键线程异常退出".to_string())?
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = tx;
        Err("非 Windows 平台不支持全局热键".to_string())
    }
}

/// 异步弹出警告对话框（独立线程，不阻塞主程序）
#[cfg(target_os = "windows")]
pub fn spawn_warning_dialog_async(message: String) {
    std::thread::spawn(move || {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;

        unsafe {
            let title: Vec<u16> = "Voice IME 警告\0".encode_utf16().collect();
            let text: Vec<u16> = format!("{}\0", message).encode_utf16().collect();
            MessageBoxW(
                0,
                text.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONWARNING | MB_TOPMOST,
            );
        }
    });
}

#[cfg(not(target_os = "windows"))]
pub fn spawn_warning_dialog_async(_message: String) {}