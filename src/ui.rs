// src/ui.rs — 语音波纹可视化窗口
//
// 屏幕上方居中，无标题栏，右键弹出菜单

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use minifb::{Window, WindowOptions, MouseButton, MouseMode};

/// 窗口尺寸
const WIN_W: usize = 120;
const WIN_H: usize = 28;

/// 波纹样式
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WaveStyle {
    Sine = 0,
    Bar = 1,
    Dot = 2,
    Flat = 3,
}

impl WaveStyle {
    fn from_u8(v: u8) -> Self {
        match v % 4 {
            0 => Self::Sine,
            1 => Self::Bar,
            2 => Self::Dot,
            _ => Self::Flat,
        }
    }
}

/// 右键菜单项 ID
const MENU_SINE: u32 = 100;
const MENU_BAR: u32 = 101;
const MENU_DOT: u32 = 102;
const MENU_FLAT: u32 = 103;
const MENU_QUIT: u32 = 200;

/// 共享状态
pub struct UiState {
    pub energy: Arc<AtomicU8>,
    pub style: Arc<AtomicU8>,
    pub quit: Arc<AtomicBool>,
}

impl UiState {
    pub fn new(quit: Arc<AtomicBool>) -> Self {
        Self {
            energy: Arc::new(AtomicU8::new(0)),
            style: Arc::new(AtomicU8::new(0)),
            quit,
        }
    }

    pub fn set_energy(&self, e: f32) {
        let v = (e.clamp(0.0, 1.0) * 255.0) as u8;
        self.energy.store(v, Ordering::Relaxed);
    }
}

/// 启动 UI 窗口（阻塞当前线程）
pub fn run_ui(state: UiState) {
    let opts = WindowOptions {
        borderless: true,
        topmost: true,
        resize: false,
        none: true,
        title: false,
        ..WindowOptions::default()
    };

    let mut window = match Window::new("", WIN_W, WIN_H, opts) {
        Ok(w) => w,
        Err(e) => {
            log::error!("窗口创建失败: {}", e);
            return;
        }
    };

    window.set_target_fps(30);

    // Windows: 定位 + 去标题 + 弹出菜单
    #[cfg(target_os = "windows")]
    let hwnd = setup_window_win32();

    let mut buf = vec![0u32; WIN_W * WIN_H];
    let mut frame: u64 = 0;
    let mut right_was_down = false;

    while window.is_open() && !state.quit.load(Ordering::Relaxed) {
        // 检测右键点击
        let right_down = window.get_mouse_down(MouseButton::Right);
        if right_down && !right_was_down {
            // 右键刚按下 → 弹出菜单
            #[cfg(target_os = "windows")]
            {
                if let Some(choice) = show_popup_menu_win32(hwnd) {
                    match choice {
                        MENU_SINE => state.style.store(0, Ordering::Relaxed),
                        MENU_BAR  => state.style.store(1, Ordering::Relaxed),
                        MENU_DOT  => state.style.store(2, Ordering::Relaxed),
                        MENU_FLAT => state.style.store(3, Ordering::Relaxed),
                        MENU_QUIT => {
                            state.quit.store(true, Ordering::Relaxed);
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }
        right_was_down = right_down;

        // 渲染
        let energy = state.energy.load(Ordering::Relaxed) as f32 / 255.0;
        let style = WaveStyle::from_u8(state.style.load(Ordering::Relaxed));
        render_frame(&mut buf, WIN_W, WIN_H, energy, style, frame);
        frame += 1;

        window.update_with_buffer(&buf, WIN_W, WIN_H).ok();
    }

    state.quit.store(true, Ordering::Relaxed);
}

// ═══════════════════════════════════════════
//  Windows API: 窗口定位 + 弹出菜单
// ═══════════════════════════════════════════

#[cfg(target_os = "windows")]
fn setup_window_win32() -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    use windows_sys::Win32::Foundation::*;

    unsafe {
        // 等待窗口创建完成
        std::thread::sleep(std::time::Duration::from_millis(100));

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let x = (screen_w - WIN_W as i32) / 2;
        let y = 6;

        // 枚举所有窗口找到我们的（通过线程ID）
        let tid = windows_sys::Win32::System::Threading::GetCurrentThreadId();
        let mut hwnd = GetTopWindow(0);

        // 简单方法：找最近创建的无标题窗口
        let mut found: isize = 0;
        for _ in 0..50 {
            if hwnd == 0 { break; }
            let mut pid: u32 = 0;
            let wtid = GetWindowThreadProcessId(hwnd, &mut pid);
            if wtid == tid {
                found = hwnd;
                break;
            }
            hwnd = GetWindow(hwnd, GW_HWNDNEXT);
        }

        if found == 0 {
            // fallback: 用 FindWindow
            let cls: Vec<u16> = "minifb_window\0".encode_utf16().collect();
            found = FindWindowW(cls.as_ptr(), std::ptr::null());
        }

        if found != 0 {
            // 去掉标题栏，设置为纯弹出窗口
            let style = WS_POPUP | WS_VISIBLE;
            SetWindowLongW(found, GWL_STYLE, style as i32);

            // 设置为工具窗口 + 置顶
            let ex_style = WS_EX_TOOLWINDOW | WS_EX_TOPMOST;
            SetWindowLongW(found, GWL_EXSTYLE, ex_style as i32);

            // 定位
            SetWindowPos(
                found,
                HWND_TOPMOST,
                x, y,
                WIN_W as i32, WIN_H as i32,
                SWP_FRAMECHANGED | SWP_SHOWWINDOW,
            );
        }

        found
    }
}

#[cfg(target_os = "windows")]
fn show_popup_menu_win32(hwnd: isize) -> Option<u32> {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::Graphics::Gdi::*;

    unsafe {
        let hmenu = CreatePopupMenu();
        if hmenu == 0 { return None; }

        // 添加菜单项
        let items: &[(u32, &str)] = &[
            (MENU_SINE, "正弦波 ～～\0"),
            (MENU_BAR,  "柱状条 ▐▌\0"),
            (MENU_DOT,  "点阵 ·•·\0"),
            (MENU_FLAT, "平直线 ──\0"),
            (0,         "-\0"),  // separator
            (MENU_QUIT, "退出\0"),
        ];

        for (id, text) in items {
            if *id == 0 {
                AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());
            } else {
                let wide: Vec<u16> = text.encode_utf16().collect();
                AppendMenuW(hmenu, MF_STRING, *id as usize, wide.as_ptr());
            }
        }

        // 获取鼠标位置
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);

        // 弹出菜单（阻塞直到选择）
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            hmenu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            pt.x, pt.y,
            0,
            hwnd,
            std::ptr::null(),
        );

        DestroyMenu(hmenu);

        if cmd > 0 { Some(cmd as u32) } else { None }
    }
}

// ═══════════════════════════════════════════
//  渲染
// ═══════════════════════════════════════════

fn render_frame(buf: &mut [u32], w: usize, h: usize, energy: f32, style: WaveStyle, frame: u64) {
    let bg = rgb(20, 20, 30);
    let fg = rgb(80, 200, 255);
    let fg_hi = rgb(255, 140, 60);

    buf.fill(bg);
    draw_border(buf, w, h, rgb(60, 60, 80));

    let mid_y = h / 2;
    let amp = energy * (mid_y as f32 - 2.0);

    match style {
        WaveStyle::Sine => {
            for x in 1..w - 1 {
                let phase = (x as f64 + frame as f64 * 2.0) * 0.15;
                let y_off = (phase.sin() * amp as f64) as i32;
                let y = (mid_y as i32 + y_off).clamp(1, h as i32 - 2) as usize;
                let color = if energy > 0.3 { fg_hi } else { fg };
                set_px(buf, w, x, y, color);
                if y > 0 { set_px(buf, w, x, y - 1, blend(color, bg, 0.5)); }
                if y < h - 1 { set_px(buf, w, x, y + 1, blend(color, bg, 0.5)); }
            }
        }
        WaveStyle::Bar => {
            let bar_w = 4;
            let num_bars = (w - 2) / (bar_w + 1);
            for i in 0..num_bars {
                let x_start = 2 + i * (bar_w + 1);
                let phase = (i as f64 + frame as f64 * 0.3) * 0.8;
                let bar_h = ((phase.sin().abs() * amp as f64) as usize).max(1);
                let color = if i % 3 == 0 { fg_hi } else { fg };
                for dx in 0..bar_w {
                    for dy in 0..bar_h.min(mid_y - 1) {
                        set_px(buf, w, x_start + dx, mid_y - dy, color);
                        set_px(buf, w, x_start + dx, mid_y + dy, color);
                    }
                }
            }
        }
        WaveStyle::Dot => {
            let dot_spacing = 6;
            let num_dots = (w - 4) / dot_spacing;
            for i in 0..num_dots {
                let x = 3 + i * dot_spacing;
                let phase = (i as f64 + frame as f64 * 0.5) * 1.2;
                let radius = ((phase.sin().abs() * amp as f64 * 0.4) as usize).max(0);
                let color = if energy > 0.2 { fg } else { rgb(40, 80, 120) };
                draw_dot(buf, w, x, mid_y, radius.min(4), color);
            }
        }
        WaveStyle::Flat => {
            for x in 2..w - 2 {
                set_px(buf, w, x, mid_y, rgb(60, 100, 140));
            }
        }
    }
}

fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}

fn blend(c1: u32, c2: u32, t: f32) -> u32 {
    let r = (((c1 >> 16) & 0xFF) as f32 * (1.0 - t) + ((c2 >> 16) & 0xFF) as f32 * t) as u8;
    let g = (((c1 >> 8) & 0xFF) as f32 * (1.0 - t) + ((c2 >> 8) & 0xFF) as f32 * t) as u8;
    let b = ((c1 & 0xFF) as f32 * (1.0 - t) + (c2 & 0xFF) as f32 * t) as u8;
    rgb(r, g, b)
}

fn set_px(buf: &mut [u32], w: usize, x: usize, y: usize, color: u32) {
    if x < w && y < buf.len() / w {
        buf[y * w + x] = color;
    }
}

fn draw_border(buf: &mut [u32], w: usize, h: usize, color: u32) {
    for x in 1..w - 1 { set_px(buf, w, x, 0, color); set_px(buf, w, x, h - 1, color); }
    for y in 1..h - 1 { set_px(buf, w, 0, y, color); set_px(buf, w, w - 1, y, color); }
}

fn draw_dot(buf: &mut [u32], w: usize, cx: usize, cy: usize, r: usize, color: u32) {
    if r == 0 { set_px(buf, w, cx, cy, color); return; }
    for dy in 0..=r {
        for dx in 0..=r {
            if dx * dx + dy * dy <= r * r {
                set_px(buf, w, cx + dx, cy + dy, color);
                if cx >= dx { set_px(buf, w, cx - dx, cy + dy, color); }
                if cy >= dy { set_px(buf, w, cx + dx, cy - dy, color); }
                if cx >= dx && cy >= dy { set_px(buf, w, cx - dx, cy - dy, color); }
            }
        }
    }
}