// src/learn.rs — 自学习模块（词频统计 + 纠错替换）
//
// 功能：
// 1. 记录每次识别结果的词频到 word_freq.json
// 2. 从 corrections.txt 加载纠错映射，自动替换识别错误
//
// corrections.txt 格式（用户可手动编辑）：
//   错误词→正确词
//   人口智能→人工智能
//   大摸型→大模型
//
// word_freq.json 格式（自动生成，不要手动编辑）：
//   {"人工智能": 15, "大模型": 8, ...}

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Result;

/// 自学习引擎
pub struct LearningEngine {
    /// 词频表: 词 → 出现次数
    word_freq: Mutex<HashMap<String, u64>>,
    /// 纠错表: 错误词 → 正确词
    corrections: HashMap<String, String>,
    /// 词频文件路径
    freq_path: PathBuf,
    /// 纠错文件路径
    corrections_path: PathBuf,
    /// 本次会话识别次数（用于定期保存）
    session_count: Mutex<u64>,
}

impl LearningEngine {
    /// 创建自学习引擎
    pub fn new(data_dir: &Path) -> Self {
        let freq_path = data_dir.join("word_freq.json");
        let corrections_path = data_dir.join("corrections.txt");

        let word_freq = Self::load_freq(&freq_path);
        let corrections = Self::load_corrections(&corrections_path);

        log::info!(
            "自学习: 词频 {} 条 | 纠错 {} 条",
            word_freq.len(),
            corrections.len()
        );

        Self {
            word_freq: Mutex::new(word_freq),
            corrections,
            freq_path,
            corrections_path,
            session_count: Mutex::new(0),
        }
    }

    /// 处理识别结果：先纠错替换，再记录词频
    pub fn process(&self, text: &str) -> String {
        // 1. 纠错替换
        let corrected = self.apply_corrections(text);

        // 2. 记录词频
        self.record_phrase(&corrected);

        corrected
    }

    /// 应用纠错映射
    fn apply_corrections(&self, text: &str) -> String {
        let mut result = text.to_string();
        for (wrong, right) in &self.corrections {
            if result.contains(wrong.as_str()) {
                log::debug!("纠错: {} → {}", wrong, right);
                result = result.replace(wrong.as_str(), right.as_str());
            }
        }
        result
    }

    /// 记录词频
    fn record_phrase(&self, text: &str) {
        if text.is_empty() {
            return;
        }

        let mut freq = self.word_freq.lock().unwrap();
        let mut count = self.session_count.lock().unwrap();

        // 按标点/空格分词记录
        let phrases: Vec<&str> = text
            .split(|c: char| c == '，' || c == '。' || c == '！' || c == '？'
                || c == '、' || c == ' ' || c == '\n')
            .filter(|s| !s.is_empty() && s.len() > 1)
            .collect();

        for phrase in phrases {
            *freq.entry(phrase.to_string()).or_insert(0) += 1;
        }

        // 同时记录完整句子
        *freq.entry(text.to_string()).or_insert(0) += 1;

        *count += 1;

        // 每 5 次识别自动保存一次
        if *count % 5 == 0 {
            drop(count);
            let _ = self.save_freq(&freq);
        }
    }

    /// 获取高频词列表（用于动态更新热词）
    pub fn top_phrases(&self, n: usize) -> Vec<(String, u64)> {
        let freq = self.word_freq.lock().unwrap();
        let mut entries: Vec<(String, u64)> = freq
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1));
        entries.truncate(n);
        entries
    }

    /// 获取词频统计
    pub fn stats(&self) -> (usize, u64) {
        let freq = self.word_freq.lock().unwrap();
        let total: u64 = freq.values().sum();
        (freq.len(), total)
    }

    /// 强制保存词频到磁盘
    pub fn flush(&self) {
        let freq = self.word_freq.lock().unwrap();
        if let Err(e) = self.save_freq(&freq) {
            log::warn!("保存词频失败: {}", e);
        }
    }

    /// 重新加载纠错文件（运行时热更新）
    pub fn reload_corrections(&mut self) {
        self.corrections = Self::load_corrections(&self.corrections_path);
        log::info!("纠错表已重新加载: {} 条", self.corrections.len());
    }

    // ─── 内部方法 ───

    fn load_freq(path: &Path) -> HashMap<String, u64> {
        if !path.exists() {
            return HashMap::new();
        }
        match std::fs::read_to_string(path) {
            Ok(content) => {
                serde_json::from_str(&content).unwrap_or_default()
            }
            Err(_) => HashMap::new(),
        }
    }

    fn save_freq(&self, freq: &HashMap<String, u64>) -> Result<()> {
        let json = serde_json::to_string_pretty(freq)?;
        std::fs::write(&self.freq_path, json)?;
        Ok(())
    }

    fn load_corrections(path: &Path) -> HashMap<String, String> {
        if !path.exists() {
            return HashMap::new();
        }
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };

        let mut map = HashMap::new();
        for line in content.lines() {
            let line = line.trim();
            // 跳过空行和注释
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            // 支持两种分隔符: → 和 =>
            let parts: Vec<&str> = if line.contains('→') {
                line.splitn(2, '→').collect()
            } else if line.contains("=>") {
                line.splitn(2, "=>").collect()
            } else {
                continue;
            };

            if parts.len() == 2 {
                let wrong = parts[0].trim().to_string();
                let right = parts[1].trim().to_string();
                if !wrong.is_empty() && !right.is_empty() {
                    map.insert(wrong, right);
                }
            }
        }
        map
    }
}

impl Drop for LearningEngine {
    fn drop(&mut self) {
        self.flush();
        log::debug!("自学习引擎已保存并释放");
    }
}