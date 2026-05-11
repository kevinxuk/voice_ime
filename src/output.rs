// src/output.rs — 键盘输出（enigo 0.6）

use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::thread;

use enigo::{Enigo, Key, Direction, Keyboard, Settings};

pub struct KeyboardOutput {
    enigo: Arc<Mutex<Enigo>>,
}

impl KeyboardOutput {
    pub fn new() -> Self {
        Self {
            enigo: Arc::new(Mutex::new(
                Enigo::new(&Settings::default()).expect("Enigo 初始化失败"),
            )),
        }
    }

    /// 发送文本（中文直接用 enigo.text()）
    pub fn send_text(&mut self, text: &str) -> anyhow::Result<()> {
        if text.is_empty() { return Ok(()); }
        let mut enigo = self.enigo.lock().unwrap();
        thread::sleep(Duration::from_millis(30));
        // enigo 0.6 的 text() 方法原生支持 Unicode / 中文
        enigo.text(text).map_err(|e| anyhow::anyhow!("键盘输出失败: {:?}", e))?;
        Ok(())
    }

    /// 发送 Ctrl+V 粘贴
    #[allow(dead_code)]
    pub fn paste(&mut self) -> anyhow::Result<()> {
        let mut enigo = self.enigo.lock().unwrap();
        enigo.key(Key::Control, Direction::Press).ok();
        thread::sleep(Duration::from_millis(20));
        enigo.key(Key::Unicode('v'), Direction::Click).ok();
        thread::sleep(Duration::from_millis(20));
        enigo.key(Key::Control, Direction::Release).ok();
        Ok(())
    }
}