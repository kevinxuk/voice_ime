// src/bigram.rs — Bigram 频率表（加载 + 查询 + 运行时更新）

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Bigram 频率表
pub struct BigramTable {
    table: HashMap<(char, char), u32>,
    path: PathBuf,
    updates: u32, // 未保存的更新次数
}

impl BigramTable {
    /// 从 bigram.bin 加载（文本格式：每行 "字1字2\tfreq"）
    pub fn load(path: &Path) -> Self {
        let mut table = HashMap::new();

        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                for line in content.lines() {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() == 2 {
                        let chars: Vec<char> = parts[0].chars().collect();
                        if chars.len() == 2 {
                            if let Ok(freq) = parts[1].parse::<u32>() {
                                table.insert((chars[0], chars[1]), freq);
                            }
                        }
                    }
                }
            }
        }

        log::info!("Bigram 加载: {} 条", table.len());
        Self {
            table,
            path: path.to_path_buf(),
            updates: 0,
        }
    }

    /// 查询二字组合频率
    pub fn freq(&self, c1: char, c2: char) -> u32 {
        self.table.get(&(c1, c2)).copied().unwrap_or(0)
    }

    /// 记录文本中的 bigram（运行时学习）
    pub fn record_text(&mut self, text: &str) {
        let chars: Vec<char> = text.chars()
            .filter(|c| is_content_char(*c))
            .collect();

        for i in 0..chars.len().saturating_sub(1) {
            let entry = self.table.entry((chars[i], chars[i + 1])).or_insert(0);
            *entry += 1;
        }
        self.updates += 1;

        // 每 20 次更新保存一次
        if self.updates >= 20 {
            self.save();
        }
    }

    /// 保存到磁盘
    pub fn save(&mut self) {
        if self.updates == 0 { return; }

        let mut content = String::with_capacity(self.table.len() * 12);
        for (&(c1, c2), &freq) in &self.table {
            content.push(c1);
            content.push(c2);
            content.push('\t');
            content.push_str(&freq.to_string());
            content.push('\n');
        }

        if let Err(e) = std::fs::write(&self.path, &content) {
            log::warn!("Bigram 保存失败: {}", e);
        } else {
            self.updates = 0;
        }
    }

    pub fn len(&self) -> usize { self.table.len() }
}

impl Drop for BigramTable {
    fn drop(&mut self) {
        self.save();
    }
}

fn is_content_char(c: char) -> bool {
    let cp = c as u32;
    // CJK 字符
    if (0x4E00..=0x9FFF).contains(&cp) || (0x3400..=0x4DBF).contains(&cp) {
        return true;
    }
    // ASCII 字母数字
    if c.is_ascii_alphanumeric() {
        return true;
    }
    false
}