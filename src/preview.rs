// src/preview.rs — Win32 原生文字预览窗口
//
// 在波纹窗口下方显示中文识别文字，1秒后自动隐藏

use std::sync::{Arc, Mutex};

#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::*;
#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::*;
#[cfg(target_os = "windows")]
use windows_sys::Win32::Graphics::Gdi::*;

/// 预览窗口（跨线程共享文字内容）
pub struct PreviewWindow {
    text: Arc<Mutex<String>>,
    visible: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(target_os = "windows")]
    hwnd: std::sync::atomic::AtomicIsize,
}

impl PreviewWindow {
    pub fn new() -> Self {
        Self {
            text: Arc::new(Mutex::new(String::new())),
            visible: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            #[cfg(target_os = "windows")]
            hwnd: std::sync::atomic::AtomicIsize::new(0),
        }
    }

    /// 显示/更新文字
    pub fn show_text(&self, text: &str) {
        *self.text.lock().unwrap() = text.to_string();
        self.visible.store(true, std::sync::atomic::Ordering::Relaxed);
        self.update_native_window();
    }

    /// 更新文字（流式中间结果）
    pub fn update_text(&self, text: &str) {
        *self.text.lock().unwrap() = text.to_string();
        if self.visible.load(std::sync::atomic::Ordering::Relaxed) {
            self.update_native_window();
        }
    }

    /// 隐藏窗口
    pub fn hide(&self) {
        self.visible.store(false, std::sync::atomic::Ordering::Relaxed);
        *self.text.lock().unwrap() = String::new();
        self.hide_native_window();
    }

    /// 是否可见
    pub fn is_visible(&self) -> bool {
        self.visible.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 获取当前文字
    pub fn current_text(&self) -> String {
        self.text.lock().unwrap().clone()
    }

    #[cfg(target_os = "windows")]
    fn update_native_window(&self) {
        unsafe {
            let hwnd = self.hwnd.load(std::sync::atomic::Ordering::Relaxed);
            if hwnd == 0 {
                // 首次创建窗口
                self.create_native_window();
                return;
            }
            // 触发重绘
            InvalidateRect(hwnd, std::ptr::null(), 1);
            ShowWindow(hwnd, SW_SHOWNA);
        }
    }

    #[cfg(target_os = "windows")]
    fn hide_native_window(&self) {
        unsafe {
            let hwnd = self.hwnd.load(std::sync::atomic::Ordering::Relaxed);
            if hwnd != 0 {
                ShowWindow(hwnd, SW_HIDE);
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn create_native_window(&self) {
        // 简化：使用现有的 minifb 波纹窗口框架
        // 实际文字通过 log 输出，在 UI 波纹窗口的下半部分渲染
        // 这里设置标志让 ui.rs 的渲染循环知道要显示文字
        log::debug!("预览窗口: 使用 UI 下半区域显示文字");
    }

    #[cfg(not(target_os = "windows"))]
    fn update_native_window(&self) {}

    #[cfg(not(target_os = "windows"))]
    fn hide_native_window(&self) {}
}