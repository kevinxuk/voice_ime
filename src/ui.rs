// src/ui.rs — 语音波纹可视化窗口
//
// 启动流程: 加载中(进度) → 就绪(平直线) → 说话(波纹动画)

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use minifb::{Window, WindowOptions, MouseButton};

/// 窗口尺寸
const WIN_W: usize = 160;
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
        match v % 4 { 0 => Self::Sine, 1 => Self::Bar, 2 => Self::Dot, _ => Self::Flat }
    }
}

/// 右键菜单 ID
const MENU_SINE: u32 = 100;
const MENU_BAR: u32 = 101;
const MENU_DOT: u32 = 102;
const MENU_FLAT: u32 = 103;
const MENU_SETTINGS: u32 = 150;
const MENU_QUIT: u32 = 200;

/// 共享状态
pub struct UiState {
    /// 当前音频能量 (0~255)
    pub energy: Arc<AtomicU8>,
    /// 波纹样式
    pub style: Arc<AtomicU8>,
    /// 程序退出
    pub quit: Arc<AtomicBool>,
    /// UI 阶段: 0=加载中 1=就绪 2=说话中 3=暂停
    pub phase: Arc<AtomicU8>,
    /// 加载进度 (0~100)
    pub progress: Arc<AtomicU8>,
    /// 黄色闪烁剩余帧数（命令执行反馈）
    pub flash_frames: Arc<AtomicU8>,
    /// 打开设置信号
    pub open_settings: Arc<AtomicBool>,
}

impl UiState {
    pub fn new(quit: Arc<AtomicBool>) -> Self {
        Self {
            energy: Arc::new(AtomicU8::new(0)),
            style: Arc::new(AtomicU8::new(0)),
            quit,
            phase: Arc::new(AtomicU8::new(0)),
            progress: Arc::new(AtomicU8::new(0)),
            flash_frames: Arc::new(AtomicU8::new(0)),
            open_settings: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_phase_loading(&self) {
        self.phase.store(0, Ordering::Relaxed);
    }
    pub fn set_progress(&self, pct: u8) {
        self.progress.store(pct.min(100), Ordering::Relaxed);
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
        Err(e) => { log::error!("窗口创建失败: {}", e); return; }
    };

    window.set_target_fps(30);

    #[cfg(target_os = "windows")]
    let hwnd = setup_window_win32();

    let mut buf = vec![0u32; WIN_W * WIN_H];
    let mut frame: u64 = 0;
    let mut right_was_down = false;

    while window.is_open() && !state.quit.load(Ordering::Relaxed) {
        // 右键菜单
        let right_down = window.get_mouse_down(MouseButton::Right);
        if right_down && !right_was_down {
            #[cfg(target_os = "windows")]
            if let Some(choice) = show_popup_menu_win32(hwnd) {
                match choice {
                    MENU_SINE => state.style.store(0, Ordering::Relaxed),
                    MENU_BAR  => state.style.store(1, Ordering::Relaxed),
                    MENU_DOT  => state.style.store(2, Ordering::Relaxed),
                    MENU_FLAT => state.style.store(3, Ordering::Relaxed),
                    MENU_SETTINGS => state.open_settings.store(true, Ordering::Relaxed),
                    MENU_QUIT => { state.quit.store(true, Ordering::Relaxed); break; }
                    _ => {}
                }
            }
        }
        right_was_down = right_down;

        // 读取状态
        let phase = state.phase.load(Ordering::Relaxed);
        let energy = state.energy.load(Ordering::Relaxed) as f32 / 255.0;
        let progress = state.progress.load(Ordering::Relaxed);
        let style = WaveStyle::from_u8(state.style.load(Ordering::Relaxed));

        // 渲染
        match phase {
            0 => render_loading(&mut buf, WIN_W, WIN_H, progress, frame),
            1 => render_ready(&mut buf, WIN_W, WIN_H, frame),
            3 => render_paused(&mut buf, WIN_W, WIN_H, frame),
            _ => render_waveform(&mut buf, WIN_W, WIN_H, energy, style, frame),
        }

        // 闪烁覆盖（命令反馈）
        let flash = state.flash_frames.load(Ordering::Relaxed);
        if flash > 0 {
            let alpha = (flash as f32 / 15.0) * 0.5; // 最多 50% 黄色
            overlay_tint(&mut buf, rgb(255, 220, 0), alpha);
            state.flash_frames.store(flash - 1, Ordering::Relaxed);
        }

        frame += 1;

        window.update_with_buffer(&buf, WIN_W, WIN_H).ok();
    }

    state.quit.store(true, Ordering::Relaxed);
}

// ═══════════════════════════════════════════
//  渲染: 加载中
// ═══════════════════════════════════════════

fn render_loading(buf: &mut [u32], w: usize, h: usize, progress: u8, frame: u64) {
    let bg = rgb(20, 20, 30);
    buf.fill(bg);
    draw_border(buf, w, h, rgb(60, 60, 80));

    // 进度条背景
    let bar_y = h / 2;
    let bar_x_start = 8;
    let bar_x_end = w - 8;
    let bar_width = bar_x_end - bar_x_start;

    // 进度条轨道
    for x in bar_x_start..bar_x_end {
        set_px(buf, w, x, bar_y, rgb(40, 40, 60));
        set_px(buf, w, x, bar_y - 1, rgb(40, 40, 60));
    }

    // 进度条填充
    let fill_end = bar_x_start + (bar_width * progress as usize) / 100;
    for x in bar_x_start..fill_end {
        let color = rgb(80, 180, 255);
        set_px(buf, w, x, bar_y, color);
        set_px(buf, w, x, bar_y - 1, color);
    }

    // 加载动画点 (脉冲)
    let dot_x = bar_x_start + ((frame as usize * 2) % bar_width);
    if dot_x < fill_end {
        set_px(buf, w, dot_x, bar_y, rgb(255, 255, 255));
        set_px(buf, w, dot_x, bar_y - 1, rgb(255, 255, 255));
    }

    // 文字提示 "加载中..." 用像素字
    draw_text_loading(buf, w, h, progress);
}

// ═══════════════════════════════════════════
//  渲染: 就绪
// ═══════════════════════════════════════════

fn render_ready(buf: &mut [u32], w: usize, h: usize, frame: u64) {
    let bg = rgb(20, 25, 20);
    buf.fill(bg);
    draw_border(buf, w, h, rgb(40, 100, 60));

    let mid_y = h / 2;

    // 绿色呼吸灯效果
    let breath = ((frame as f64 * 0.05).sin() * 0.3 + 0.7) as f32;
    let g = (180.0 * breath) as u8;
    let color = rgb(40, g, 80);

    // 中间平直线 + 轻微起伏
    for x in 4..w - 4 {
        let wobble = ((x as f64 + frame as f64 * 0.5) * 0.1).sin() * 1.0;
        let y = (mid_y as f64 + wobble) as usize;
        set_px(buf, w, x, y, color);
    }

    // "就绪" 指示点
    let dot_color = rgb(60, 220, 100);
    set_px(buf, w, 3, mid_y, dot_color);
    set_px(buf, w, 4, mid_y, dot_color);
    set_px(buf, w, 3, mid_y - 1, dot_color);
    set_px(buf, w, 4, mid_y - 1, dot_color);
}

// ═══════════════════════════════════════════
//  渲染: 暂停中
// ═══════════════════════════════════════════

fn render_paused(buf: &mut [u32], w: usize, h: usize, frame: u64) {
    let bg = rgb(40, 15, 15);
    buf.fill(bg);
    draw_border(buf, w, h, rgb(120, 40, 40));

    let mid_y = h / 2;

    // 暗红色呼吸灯
    let breath = ((frame as f64 * 0.04).sin() * 0.3 + 0.7) as f32;
    let r = (200.0 * breath) as u8;
    let line_color = rgb(r.saturating_sub(100), 30, 30);

    // 中间暗红平直线
    for x in 14..w - 4 {
        set_px(buf, w, x, mid_y, line_color);
    }

    // 左侧 ⏸ 图标（两条竖线）
    let pause_color = rgb(230, 80, 80);
    let icon_x = 5;
    let icon_top = mid_y.saturating_sub(5);
    let icon_bot = (mid_y + 5).min(h - 2);
    for y in icon_top..=icon_bot {
        set_px(buf, w, icon_x, y, pause_color);
        set_px(buf, w, icon_x + 1, y, pause_color);
        set_px(buf, w, icon_x + 4, y, pause_color);
        set_px(buf, w, icon_x + 5, y, pause_color);
    }
}

// ═══════════════════════════════════════════
//  渲染: 波纹 (说话中)
// ═══════════════════════════════════════════

fn render_waveform(buf: &mut [u32], w: usize, h: usize, energy: f32, style: WaveStyle, frame: u64) {
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

// ═══════════════════════════════════════════
//  像素文字: "加载中 XX%"
// ═══════════════════════════════════════════

fn draw_text_loading(buf: &mut [u32], w: usize, h: usize, progress: u8) {
    // 简单的 3x5 数字像素字体绘制进度百分比
    let text_y = h / 2 - 5; // 进度条上方
    let text_color = rgb(160, 200, 240);

    // 绘制百分比数字 (居中)
    let pct_str = format!("{}%", progress);
    let char_w = 4;
    let total_w = pct_str.len() * char_w;
    let start_x = (w - total_w) / 2;

    for (i, ch) in pct_str.chars().enumerate() {
        let x = start_x + i * char_w;
        draw_mini_char(buf, w, x, text_y, ch, text_color);
    }
}

/// 3x5 像素迷你字符
fn draw_mini_char(buf: &mut [u32], w: usize, x: usize, y: usize, ch: char, color: u32) {
    let pattern: [u8; 5] = match ch {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b001, 0b001, 0b001],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        '%' => [0b101, 0b001, 0b010, 0b100, 0b101],
        _ => [0; 5],
    };

    for row in 0..5 {
        for col in 0..3 {
            if (pattern[row] >> (2 - col)) & 1 == 1 {
                set_px(buf, w, x + col, y + row, color);
            }
        }
    }
}

// ═══════════════════════════════════════════
//  Windows API
// ═══════════════════════════════════════════

#[cfg(target_os = "windows")]
fn setup_window_win32() -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    unsafe {
        std::thread::sleep(std::time::Duration::from_millis(200));

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let x = (screen_w - WIN_W as i32) / 2;
        let y = 6;

        let tid = windows_sys::Win32::System::Threading::GetCurrentThreadId();
        let mut hwnd = GetTopWindow(0);
        let mut found: isize = 0;

        for _ in 0..100 {
            if hwnd == 0 { break; }
            let mut pid: u32 = 0;
            let wtid = GetWindowThreadProcessId(hwnd, &mut pid);
            if wtid == tid && IsWindowVisible(hwnd) != 0 {
                found = hwnd;
                break;
            }
            hwnd = GetWindow(hwnd, GW_HWNDNEXT);
        }

        if found == 0 {
            let cls: Vec<u16> = "minifb_window\0".encode_utf16().collect();
            found = FindWindowW(cls.as_ptr(), std::ptr::null());
        }

        if found != 0 {
            let style = WS_POPUP | WS_VISIBLE;
            SetWindowLongW(found, GWL_STYLE, style as i32);

            let ex_style = WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED;
            SetWindowLongW(found, GWL_EXSTYLE, ex_style as i32);

            SetLayeredWindowAttributes(found, 0, 179, LWA_ALPHA);

            SetWindowPos(found, HWND_TOPMOST, x, y, WIN_W as i32, WIN_H as i32,
                SWP_FRAMECHANGED | SWP_SHOWWINDOW);
        }

        found
    }
}

#[cfg(target_os = "windows")]
fn show_popup_menu_win32(hwnd: isize) -> Option<u32> {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    unsafe {
        let hmenu = CreatePopupMenu();
        if hmenu == 0 { return None; }

        let items: &[(u32, &str)] = &[
            (MENU_SINE, "正弦波 ～～\0"),
            (MENU_BAR,  "柱状条 ▐▌\0"),
            (MENU_DOT,  "点阵 ·•·\0"),
            (MENU_FLAT, "平直线 ──\0"),
            (0, "-\0"),
            (MENU_SETTINGS, "设置...\0"),
            (0, "-\0"),
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

        let mut pt = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(hmenu, TPM_RETURNCMD | TPM_RIGHTBUTTON,
            pt.x, pt.y, 0, hwnd, std::ptr::null());
        DestroyMenu(hmenu);

        if cmd > 0 { Some(cmd as u32) } else { None }
    }
}

// ═══════════════════════════════════════════
//  像素工具
// ═══════════════════════════════════════════

fn rgb(r: u8, g: u8, b: u8) -> u32 { (r as u32) << 16 | (g as u32) << 8 | b as u32 }

fn blend(c1: u32, c2: u32, t: f32) -> u32 {
    let r = (((c1 >> 16) & 0xFF) as f32 * (1.0 - t) + ((c2 >> 16) & 0xFF) as f32 * t) as u8;
    let g = (((c1 >> 8) & 0xFF) as f32 * (1.0 - t) + ((c2 >> 8) & 0xFF) as f32 * t) as u8;
    let b = ((c1 & 0xFF) as f32 * (1.0 - t) + (c2 & 0xFF) as f32 * t) as u8;
    rgb(r, g, b)
}

/// 全屏黄色叠加（命令反馈闪烁）
fn overlay_tint(buf: &mut [u32], tint: u32, alpha: f32) {
    let a = alpha.clamp(0.0, 1.0);
    for px in buf.iter_mut() {
        *px = blend(*px, tint, a);
    }
}

fn set_px(buf: &mut [u32], w: usize, x: usize, y: usize, color: u32) {
    if x < w && y < buf.len() / w { buf[y * w + x] = color; }
}

fn draw_border(buf: &mut [u32], w: usize, h: usize, color: u32) {
    for x in 1..w - 1 { set_px(buf, w, x, 0, color); set_px(buf, w, x, h - 1, color); }
    for y in 1..h - 1 { set_px(buf, w, 0, y, color); set_px(buf, w, w - 1, y, color); }
}

fn draw_dot(buf: &mut [u32], w: usize, cx: usize, cy: usize, r: usize, color: u32) {
    if r == 0 { set_px(buf, w, cx, cy, color); return; }
    for dy in 0..=r { for dx in 0..=r {
        if dx * dx + dy * dy <= r * r {
            set_px(buf, w, cx + dx, cy + dy, color);
            if cx >= dx { set_px(buf, w, cx - dx, cy + dy, color); }
            if cy >= dy { set_px(buf, w, cx + dx, cy - dy, color); }
            if cx >= dx && cy >= dy { set_px(buf, w, cx - dx, cy - dy, color); }
        }
    }}
}