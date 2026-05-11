// src/hotkey.rs — 全局热键（按住/松开检测）
//
// 使用 GetAsyncKeyState 轮询检测热键按住和松开

use std::sync::mpsc::Sender;
use std::time::Duration;
use std::thread;

#[derive(Debug, Clone, Copy)]
pub enum HotkeyEvent {
    KeyDown,  // 热键按下 → 开始录音
    KeyUp,    // 热键松开 → 停止录音
}

/// 解析热键字符串为虚拟键码列表
/// "Ctrl+Alt+V" → vec![(VK_CONTROL, true), (VK_MENU, true), (0x56, false)]
fn parse_hotkey(s: &str) -> Result<(Vec<i32>, i32), String> {
    let mut modifiers: Vec<i32> = Vec::new();
    let mut main_vk: Option<i32> = None;

    for part in s.split('+') {
        let p = part.trim().to_lowercase();
        match p.as_str() {
            "ctrl" | "control" => modifiers.push(0x11), // VK_CONTROL
            "alt" => modifiers.push(0x12),              // VK_MENU
            "shift" => modifiers.push(0x10),            // VK_SHIFT
            "win" | "meta" | "super" => modifiers.push(0x5B), // VK_LWIN
            "" => {}
            other => {
                if main_vk.is_some() {
                    return Err(format!("多个主键: {}", s));
                }
                main_vk = Some(parse_vk(other)?);
            }
        }
    }

    match main_vk {
        Some(vk) => Ok((modifiers, vk)),
        None => Err(format!("缺少主键: {}", s)),
    }
}

fn parse_vk(s: &str) -> Result<i32, String> {
    let lower = s.to_lowercase();
    if lower.chars().count() == 1 {
        let ch = lower.chars().next().unwrap();
        if ch.is_ascii_alphabetic() {
            return Ok(0x41 + (ch.to_ascii_uppercase() as i32 - 'A' as i32));
        }
        if ch.is_ascii_digit() {
            return Ok(0x30 + (ch as i32 - '0' as i32));
        }
    }
    match lower.as_str() {
        "f1" => Ok(0x70), "f2" => Ok(0x71), "f3" => Ok(0x72), "f4" => Ok(0x73),
        "f5" => Ok(0x74), "f6" => Ok(0x75), "f7" => Ok(0x76), "f8" => Ok(0x77),
        "f9" => Ok(0x78), "f10" => Ok(0x79), "f11" => Ok(0x7A), "f12" => Ok(0x7B),
        "space" => Ok(0x20), "enter" | "return" => Ok(0x0D),
        "tab" => Ok(0x09), "escape" | "esc" => Ok(0x1B),
        _ => Err(format!("未知按键: {}", s)),
    }
}

/// 启动热键轮询线程（检测按住和松开）
pub fn spawn_hotkey_thread(
    hotkey: &str,
    tx: Sender<HotkeyEvent>,
) -> Result<(), String> {
    let (modifiers, main_vk) = parse_hotkey(hotkey)?;
    let hotkey_str = hotkey.to_string();

    thread::spawn(move || {
        let mut was_pressed = false;

        loop {
            #[cfg(target_os = "windows")]
            let all_pressed = unsafe {
                use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
                let mods_ok = modifiers.iter().all(|&vk| GetAsyncKeyState(vk) < 0);
                let main_ok = GetAsyncKeyState(main_vk) < 0;
                mods_ok && main_ok
            };

            #[cfg(not(target_os = "windows"))]
            let all_pressed = false;

            if all_pressed && !was_pressed {
                let _ = tx.send(HotkeyEvent::KeyDown);
            }
            if !all_pressed && was_pressed {
                let _ = tx.send(HotkeyEvent::KeyUp);
            }
            was_pressed = all_pressed;

            thread::sleep(Duration::from_millis(20));
        }
    });

    log::info!("热键轮询启动: {} (按住=录音, 松开=停止)", hotkey_str);
    Ok(())
}

/// 弹出异步警告对话框
#[cfg(target_os = "windows")]
pub fn spawn_warning_dialog_async(message: String) {
    thread::spawn(move || unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let title: Vec<u16> = "Voice IME\0".encode_utf16().collect();
        let text: Vec<u16> = format!("{}\0", message).encode_utf16().collect();
        MessageBoxW(0, text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONWARNING | MB_TOPMOST);
    });
}

#[cfg(not(target_os = "windows"))]
pub fn spawn_warning_dialog_async(_: String) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hotkey_ctrl_alt_v() {
        let (mods, vk) = parse_hotkey("Ctrl+Alt+V").unwrap();
        assert_eq!(mods, vec![0x11, 0x12]); // VK_CONTROL, VK_MENU
        assert_eq!(vk, 0x56); // V
    }

    #[test]
    fn test_parse_hotkey_f1() {
        let (mods, vk) = parse_hotkey("F1").unwrap();
        assert!(mods.is_empty());
        assert_eq!(vk, 0x70);
    }

    #[test]
    fn test_parse_hotkey_shift_a() {
        let (mods, vk) = parse_hotkey("Shift+A").unwrap();
        assert_eq!(mods, vec![0x10]); // VK_SHIFT
        assert_eq!(vk, 0x41); // A
    }

    #[test]
    fn test_parse_hotkey_no_main_key() {
        assert!(parse_hotkey("Ctrl+Alt").is_err());
    }
}