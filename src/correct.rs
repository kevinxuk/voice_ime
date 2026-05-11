// src/correct.rs — 统一纠错引擎
//
// 纠错流水线:
// 1. corrections.txt 精确替换（已有的错→对映射）
// 2. Bigram + 同音字纠错（统计方法）

use std::collections::HashMap;
use std::path::Path;

use crate::bigram::BigramTable;

/// 纠错引擎
pub struct CorrectionEngine {
    corrections: HashMap<String, String>,
    homophones: HashMap<char, Vec<char>>,
    pub bigram: BigramTable,
}

impl CorrectionEngine {
    pub fn new(model_dir: &Path) -> Self {
        let corrections = load_corrections(&model_dir.join("corrections.txt"));
        let homophones = load_homophones(&model_dir.join("homophones.txt"));
        let bigram = BigramTable::load(&model_dir.join("bigram.bin"));

        log::info!(
            "纠错引擎: corrections {} 条 | homophones {} 组 | bigram {} 条",
            corrections.len(), homophones.len(), bigram.len()
        );

        Self { corrections, homophones, bigram }
    }

    /// 对整句做纠错（corrections + bigram 同音字）
    pub fn correct(&self, text: &str) -> String {
        // 1. corrections.txt 精确替换
        let text = self.apply_corrections(text);

        // 2. Bigram 同音字纠错
        let text = self.bigram_correct(&text);

        text
    }

    /// 记录用户输出（更新 bigram 频率）
    pub fn record(&mut self, text: &str) {
        self.bigram.record_text(text);
    }

    /// 保存 bigram 数据
    pub fn flush(&mut self) {
        self.bigram.save();
    }

    // ─── 内部方法 ───

    fn apply_corrections(&self, text: &str) -> String {
        let mut result = text.to_string();
        for (wrong, right) in &self.corrections {
            if result.contains(wrong.as_str()) {
                result = result.replace(wrong.as_str(), right.as_str());
            }
        }
        result
    }

    fn bigram_correct(&self, text: &str) -> String {
        let chars: Vec<char> = text.chars().collect();
        if chars.len() < 2 { return text.to_string(); }

        let mut result = chars.clone();

        for i in 0..result.len() - 1 {
            let c1 = result[i];
            let c2 = result[i + 1];

            // 跳过标点和空格
            if !is_cjk(c1) || !is_cjk(c2) { continue; }

            let current_freq = self.bigram.freq(c1, c2);

            // 频率已经很高，无需纠正
            if current_freq > 500 { continue; }

            // 尝试替换 c2
            if let Some(candidates) = self.homophones.get(&c2) {
                let mut best_char = c2;
                let mut best_freq = current_freq;
                for &alt in candidates {
                    if alt == c2 { continue; }
                    let alt_freq = self.bigram.freq(c1, alt);
                    // 候选频率必须高 10 倍以上才替换
                    if alt_freq > best_freq.max(1) * 10 {
                        best_char = alt;
                        best_freq = alt_freq;
                    }
                }
                if best_char != c2 {
                    log::debug!("Bigram 纠错: {}{} → {}{} (freq {} → {})",
                        c1, c2, c1, best_char, current_freq, best_freq);
                    result[i + 1] = best_char;
                }
            }

            // 尝试替换 c1（如果上面没替换 c2）
            if result[i + 1] == c2 {
                if let Some(candidates) = self.homophones.get(&c1) {
                    let mut best_char = c1;
                    let mut best_freq = self.bigram.freq(c1, c2);
                    for &alt in candidates {
                        if alt == c1 { continue; }
                        let alt_freq = self.bigram.freq(alt, c2);
                        if alt_freq > best_freq.max(1) * 10 {
                            best_char = alt;
                            best_freq = alt_freq;
                        }
                    }
                    if best_char != c1 {
                        log::debug!("Bigram 纠错: {}{} → {}{} (freq → {})",
                            c1, c2, best_char, c2, best_freq);
                        result[i] = best_char;
                    }
                }
            }
        }

        result.iter().collect()
    }
}

fn is_cjk(c: char) -> bool {
    let cp = c as u32;
    (0x4E00..=0x9FFF).contains(&cp) || (0x3400..=0x4DBF).contains(&cp)
}

fn load_corrections(path: &Path) -> HashMap<String, String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let parts: Vec<&str> = if line.contains('→') {
            line.splitn(2, '→').collect()
        } else if line.contains("=>") {
            line.splitn(2, "=>").collect()
        } else { continue };

        if parts.len() == 2 {
            // 支持多变体: "A|B|C→D"
            let right = parts[1].trim().to_string();
            for variant in parts[0].split('|') {
                let wrong = variant.trim().to_string();
                if !wrong.is_empty() && !right.is_empty() {
                    map.insert(wrong, right.clone());
                }
            }
        }
    }
    map
}

fn load_homophones(path: &Path) -> HashMap<char, Vec<char>> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let mut map: HashMap<char, Vec<char>> = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let chars: Vec<char> = line.chars().collect();
        if chars.len() < 2 { continue; }
        // 这一行的所有字互为同音候选
        for &c in &chars {
            let others: Vec<char> = chars.iter().filter(|&&x| x != c).copied().collect();
            map.entry(c).or_default().extend(others);
        }
    }
    // 去重
    for v in map.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    map
}