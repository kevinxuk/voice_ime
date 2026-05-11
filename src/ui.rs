// src/ui.rs — 语音波纹可视化窗口
//
// 屏幕上方居中显示约 100x20 像素的小窗口（约 0.5cm x 1cm @96dpi）
// 有语音输入时显示波纹动画，右键菜单可切换样式或退出

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use minifb::{Window, WindowOptions, Key, MouseButton, MouseMode, Menu};

/// 窗口尺寸（像素）
const WIN_W: usize = 120;
const WIN_H: usize = 28;

/// 波纹样式
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WaveStyle {
    Sine = 0,       // 正弦波
    Bar = 1,        // 柱状条
    Dot = 2,        // 点阵脉冲
    Flat = 3,       // 平直线（静默状态）
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

    fn name(&self) -> &'static str {
        match self {
            Self::Sine => "正弦波",
            Self::Bar  => "柱状条",
            Self::Dot  => "点阵脉冲",
            Self::Flat => "平直线",
        }
    }
}

/// 右键菜单项 ID
const MENU_SINE: usize = 100;
const MENU_BAR: usize = 101;
const MENU_DOT: usize = 102;
const MENU_FLAT: usize = 103;
const MENU_QUIT: usize = 200;

/// 共享状态
pub struct UiState {
    /// 当前音频能量 (0.0~1.0)
    pub energy: Arc<AtomicU8>,  // 0~255 映射 0.0~1.0
    /// 波纹样式
    pub style: Arc<AtomicU8>,
    /// 程序退出信号
    pub quit: Arc<AtomicBool>,
}

impl UiState {
    pub fn new(quit: Arc<AtomicBool>) -> Self {
        Self {
            energy: Arc::new(AtomicU8::new(0)),
            style: Arc::new(AtomicU8::new(0)), // 默认正弦波
            quit,
        }
    }

    /// 设置当前音量 (0.0~1.0)
    pub fn set_energy(&self, e: f32) {
        let v = (e.clamp(0.0, 1.0) * 255.0) as u8;
        self.energy.store(v, Ordering::Relaxed);
    }
}

/// 启动 UI 窗口（阻塞当前线程直到窗口关闭）
pub fn run_ui(state: UiState) {
    let mut opts = WindowOptions::default();
    opts.borderless = true;
    opts.topmost = true;
    opts.resize = false;
    opts.none = true; // 无标题栏

    let mut window = match Window::new("Voice IME", WIN_W, WIN_H, opts) {
        Ok(w) => w,
        Err(e) => {
            log::error!("创建窗口失败: {}", e);
            return;
        }
    };

    // 设置窗口位置：屏幕上方居中
    position_window_top_center(&mut window);

    // 添加右键菜单
    let mut menu = Menu::new("样式").unwrap();
    menu.add_item("正弦波 ○～", MENU_SINE).build();
    menu.add_item("柱状条 ▐▌", MENU_BAR).build();
    menu.add_item("点阵脉冲 ·•", MENU_DOT).build();
    menu.add_item("平直线 ─", MENU_FLAT).build();
    menu.add_separator();
    menu.add_item("退出", MENU_QUIT).build();
    window.add_menu(&menu);

    window.set_target_fps(30);

    let mut buf = vec![0u32; WIN_W * WIN_H];
    let mut frame: u64 = 0;

    while window.is_open() && !state.quit.load(Ordering::Relaxed) {
        // 检查菜单事件
        if let Some(menu_id) = window.is_menu_pressed() {
            match menu_id {
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

        // 读取状态
        let energy = state.energy.load(Ordering::Relaxed) as f32 / 255.0;
        let style = WaveStyle::from_u8(state.style.load(Ordering::Relaxed));

        // 渲染波纹
        render_frame(&mut buf, WIN_W, WIN_H, energy, style, frame);
        frame += 1;

        window.update_with_buffer(&buf, WIN_W, WIN_H).ok();
    }

    // 窗口关闭时也发送退出信号
    state.quit.store(true, Ordering::Relaxed);
}

/// 渲染一帧波纹
fn render_frame(buf: &mut [u32], w: usize, h: usize, energy: f32, style: WaveStyle, frame: u64) {
    let bg = rgb(20, 20, 30);       // 深色背景
    let fg = rgb(80, 200, 255);     // 青色前景
    let fg_hi = rgb(255, 140, 60);  // 橙色高亮

    // 清空背景
    buf.fill(bg);

    // 圆角边框
    draw_border(buf, w, h, rgb(60, 60, 80));

    let mid_y = h / 2;
    let amp = energy * (mid_y as f32 - 2.0); // 波幅

    match style {
        WaveStyle::Sine => {
            for x in 1..w - 1 {
                let phase = (x as f64 + frame as f64 * 2.0) * 0.15;
                let y_off = (phase.sin() * amp as f64) as i32;
                let y = (mid_y as i32 + y_off).clamp(1, h as i32 - 2) as usize;
                let color = if energy > 0.3 { fg_hi } else { fg };
                set_px(buf, w, x, y, color);
                // 加粗
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
                draw_dot(buf, w, h, x, mid_y, radius.min(4), color);
            }
        }
        WaveStyle::Flat => {
            // 静默时只画一条细线
            for x in 2..w - 2 {
                set_px(buf, w, x, mid_y, rgb(60, 100, 140));
            }
        }
    }
}

// ─── 像素工具函数 ───

fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}

fn blend(c1: u32, c2: u32, t: f32) -> u32 {
    let r1 = ((c1 >> 16) & 0xFF) as f32;
    let g1 = ((c1 >> 8) & 0xFF) as f32;
    let b1 = (c1 & 0xFF) as f32;
    let r2 = ((c2 >> 16) & 0xFF) as f32;
    let g2 = ((c2 >> 8) & 0xFF) as f32;
    let b2 = (c2 & 0xFF) as f32;
    let r = (r1 * (1.0 - t) + r2 * t) as u8;
    let g = (g1 * (1.0 - t) + g2 * t) as u8;
    let b = (b1 * (1.0 - t) + b2 * t) as u8;
    rgb(r, g, b)
}

fn set_px(buf: &mut [u32], w: usize, x: usize, y: usize, color: u32) {
    if x < w && y < buf.len() / w {
        buf[y * w + x] = color;
    }
}

fn draw_border(buf: &mut [u32], w: usize, h: usize, color: u32) {
    for x in 1..w - 1 {
        set_px(buf, w, x, 0, color);
        set_px(buf, w, x, h - 1, color);
    }
    for y in 1..h - 1 {
        set_px(buf, w, 0, y, color);
        set_px(buf, w, w - 1, y, color);
    }
}

fn draw_dot(buf: &mut [u32], w: usize, h: usize, cx: usize, cy: usize, r: usize, color: u32) {
    if r == 0 {
        set_px(buf, w, cx, cy, color);
        return;
    }
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

/// 将窗口定位到屏幕上方居中
fn position_window_top_center(_window: &mut Window) {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        use windows_sys::Win32::Foundation::*;

        // 获取屏幕宽度
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let x = (screen_w - WIN_W as i32) / 2;
        let y = 8; // 距顶部 8 像素

        // 查找窗口句柄（通过标题）
        let title = "Voice IME\0".encode_utf16().collect::<Vec<u16>>();
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if hwnd != 0 {
            // 设置窗口位置和属性
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                x, y,
                WIN_W as i32, WIN_H as i32,
                SWP_NOSIZE | SWP_SHOWWINDOW,
            );

            // 设置为工具窗口样式（不出现在任务栏）
            let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
            SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_TOOLWINDOW as i32);
        }
    }
}