// src/preview.rs — Win32 原生文字预览窗口
//
// 在波纹窗口正下方显示中文识别文字
// 超过窗口宽度自动从右向左滚动
// 独立线程运行消息循环

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

/// 预览窗口宽度/高度
const PREVIEW_W: i32 = 340;
const PREVIEW_H: i32 = 26;
/// 滚动速度（像素/tick，tick=50ms）
const SCROLL_SPEED: i32 = 2;
/// 自定义消息 ID
const WM_UPDATE_TEXT: u32 = 0x0400 + 100; // WM_USER + 100
const WM_HIDE_PREVIEW: u32 = 0x0400 + 101;
const WM_SHOW_PREVIEW: u32 = 0x0400 + 102;
/// 滚动定时器 ID
const TIMER_SCROLL: usize = 1;

pub struct PreviewWindow {
    text: Arc<Mutex<String>>,
    visible: Arc<AtomicBool>,
    hwnd: Arc<AtomicIsize>,
    ready: Arc<AtomicBool>,
}

impl PreviewWindow {
    pub fn new() -> Self {
        let pw = Self {
            text: Arc::new(Mutex::new(String::new())),
            visible: Arc::new(AtomicBool::new(false)),
            hwnd: Arc::new(AtomicIsize::new(0)),
            ready: Arc::new(AtomicBool::new(false)),
        };

        // 启动 Win32 窗口线程
        #[cfg(target_os = "windows")]
        {
            let text = pw.text.clone();
            let hwnd = pw.hwnd.clone();
            let ready = pw.ready.clone();
            std::thread::spawn(move || {
                run_preview_thread(text, hwnd, ready);
            });
            // 等窗口创建完成
            while !pw.ready.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }

        pw
    }

    pub fn show_text(&self, text: &str) {
        *self.text.lock().unwrap() = text.to_string();
        self.visible.store(true, Ordering::Relaxed);
        #[cfg(target_os = "windows")]
        self.post_message(WM_SHOW_PREVIEW);
    }

    pub fn update_text(&self, text: &str) {
        *self.text.lock().unwrap() = text.to_string();
        #[cfg(target_os = "windows")]
        if self.visible.load(Ordering::Relaxed) {
            self.post_message(WM_UPDATE_TEXT);
        }
    }

    pub fn hide(&self) {
        self.visible.store(false, Ordering::Relaxed);
        *self.text.lock().unwrap() = String::new();
        #[cfg(target_os = "windows")]
        self.post_message(WM_HIDE_PREVIEW);
    }

    pub fn is_visible(&self) -> bool {
        self.visible.load(Ordering::Relaxed)
    }

    pub fn current_text(&self) -> String {
        self.text.lock().unwrap().clone()
    }

    #[cfg(target_os = "windows")]
    fn post_message(&self, msg: u32) {
        let h = self.hwnd.load(Ordering::Relaxed);
        if h != 0 {
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(h, msg, 0, 0);
            }
        }
    }
}

// ═══════════════════════════════════════════
//  Win32 窗口线程
// ═══════════════════════════════════════════

#[cfg(target_os = "windows")]
fn run_preview_thread(
    text: Arc<Mutex<String>>,
    hwnd_out: Arc<AtomicIsize>,
    ready: Arc<AtomicBool>,
) {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

    unsafe {
        // 注册窗口类
        let class_name: Vec<u16> = "VoiceIME_Preview\0".encode_utf16().collect();
        let hinstance = GetModuleHandleW(std::ptr::null());

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(preview_wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: 0,
            hCursor: LoadCursorW(0, IDC_ARROW),
            hbrBackground: 0, // 我们自己画背景
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&wc);

        // 计算位置：屏幕上方居中，在波纹窗口下方
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let x = (screen_w - PREVIEW_W) / 2;
        let y = 36; // 波纹窗口高度(28) + 间距(8)

        // 创建窗口（初始隐藏）
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_NOACTIVATE,
            class_name.as_ptr(),
            std::ptr::null(), // 无标题
            WS_POPUP,         // 无边框弹出
            x, y,
            PREVIEW_W, PREVIEW_H,
            0, 0, hinstance, std::ptr::null(),
        );

        if hwnd == 0 {
            log::error!("预览窗口创建失败");
            ready.store(true, Ordering::Relaxed);
            return;
        }

        // 透明度 85%
        SetLayeredWindowAttributes(hwnd, 0, 216, LWA_ALPHA);

        // 将 text Arc 存入窗口 user data
        let state = Box::new(PreviewState {
            text,
            scroll_offset: 0,
            text_width: 0,
            font: create_font(),
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);

        hwnd_out.store(hwnd, Ordering::Relaxed);
        ready.store(true, Ordering::Relaxed);
        log::debug!("预览窗口已创建: hwnd={}", hwnd);

        // 消息循环
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(target_os = "windows")]
struct PreviewState {
    text: Arc<Mutex<String>>,
    scroll_offset: i32,
    text_width: i32,
    font: isize, // HFONT
}

#[cfg(target_os = "windows")]
unsafe fn create_font() -> isize {
    use windows_sys::Win32::Graphics::Gdi::*;
    let face: Vec<u16> = "Microsoft YaHei\0".encode_utf16().collect();
    CreateFontW(
        -14,                    // height
        0,                      // width
        0,                      // escapement
        0,                      // orientation
        FW_NORMAL as i32,       // weight
        0,                      // italic
        0,                      // underline
        0,                      // strikeout
        DEFAULT_CHARSET as u32, // charset
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        DEFAULT_PITCH as u32,
        face.as_ptr(),
    ) as isize
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn preview_wndproc(
    hwnd: isize,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::Foundation::*;

    match msg {
        WM_SHOW_PREVIEW => {
            let state = get_state(hwnd);
            if let Some(s) = state {
                s.scroll_offset = 0;
                s.text_width = 0;
                // 启动滚动定时器（50ms）
                SetTimer(hwnd, TIMER_SCROLL, 50, None);
            }
            ShowWindow(hwnd, SW_SHOWNA);
            InvalidateRect(hwnd, std::ptr::null(), 1);
            0
        }

        WM_UPDATE_TEXT => {
            let state = get_state(hwnd);
            if let Some(s) = state {
                s.scroll_offset = 0;
                s.text_width = 0;
            }
            InvalidateRect(hwnd, std::ptr::null(), 1);
            0
        }

        WM_HIDE_PREVIEW => {
            KillTimer(hwnd, TIMER_SCROLL);
            ShowWindow(hwnd, SW_HIDE);
            0
        }

        WM_TIMER => {
            if wparam == TIMER_SCROLL {
                let state = get_state(hwnd);
                if let Some(s) = state {
                    // 如果文字宽度超出窗口，自动滚动
                    if s.text_width > PREVIEW_W - 16 {
                        s.scroll_offset += SCROLL_SPEED;
                        // 滚动到末尾后回到起点
                        let max_scroll = s.text_width - (PREVIEW_W - 16) + 40;
                        if s.scroll_offset > max_scroll {
                            s.scroll_offset = 0;
                        }
                        InvalidateRect(hwnd, std::ptr::null(), 1);
                    }
                }
            }
            0
        }

        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);

            let state = get_state(hwnd);
            if let Some(s) = state {
                // 背景
                let bg_brush = CreateSolidBrush(0x001E1E14); // RGB(20, 30, 30)
                let mut rc = RECT { left: 0, top: 0, right: PREVIEW_W, bottom: PREVIEW_H };
                FillRect(hdc, &rc, bg_brush);
                DeleteObject(bg_brush as isize);

                // 边框
                let pen = CreatePen(PS_SOLID as i32, 1, 0x00504030); // RGB(48, 64, 80)
                let old_pen = SelectObject(hdc, pen as isize);
                MoveToEx(hdc, 0, 0, std::ptr::null_mut());
                LineTo(hdc, PREVIEW_W - 1, 0);
                LineTo(hdc, PREVIEW_W - 1, PREVIEW_H - 1);
                LineTo(hdc, 0, PREVIEW_H - 1);
                LineTo(hdc, 0, 0);
                SelectObject(hdc, old_pen);
                DeleteObject(pen as isize);

                // 文字
                let text = s.text.lock().unwrap().clone();
                if !text.is_empty() {
                    let wide: Vec<u16> = text.encode_utf16().collect();

                    let old_font = SelectObject(hdc, s.font);
                    SetBkMode(hdc, TRANSPARENT as i32);
                    SetTextColor(hdc, 0x00F0D080); // RGB(128, 208, 240) — 青色

                    // 计算文字宽度
                    let mut size: SIZE = std::mem::zeroed();
                    GetTextExtentPoint32W(hdc, wide.as_ptr(), wide.len() as i32, &mut size);
                    s.text_width = size.cx;

                    // 绘制文字（带滚动偏移）
                    let text_x = 8 - s.scroll_offset;
                    let text_y = (PREVIEW_H - size.cy) / 2;
                    TextOutW(hdc, text_x, text_y, wide.as_ptr(), wide.len() as i32);

                    SelectObject(hdc, old_font);
                }
            }

            EndPaint(hwnd, &ps);
            0
        }

        WM_DESTROY => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            if ptr != 0 {
                let state = Box::from_raw(ptr as *mut PreviewState);
                DeleteObject(state.font);
            }
            PostQuitMessage(0);
            0
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(target_os = "windows")]
unsafe fn get_state(hwnd: isize) -> Option<&'static mut PreviewState> {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
    if ptr == 0 {
        None
    } else {
        Some(&mut *(ptr as *mut PreviewState))
    }
}