// src/correct.rs — 统一纠错引擎 + 自学习
//
// 纠错流水线:
// 1. corrections.txt + auto_corrections.txt 精确替换
// 2. Bigram + 同音字纠错（统计方法）
//
// 自学习:
// - learn_from_correction(original, corrected) 对比差异
// - 自动追加 auto_corrections.txt
// - 调整 bigram 频率
// - 追加热词

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::bigram::BigramTable;

pub struct CorrectionEngine {
    corrections: HashMap<String, String>,
    homophones: HashMap<char, Vec<char>>,
    pub bigram: BigramTable,
    model_dir: PathBuf,
}

impl CorrectionEngine {
    pub fn new(model_dir: &Path) -> Self {
        let mut corrections = load_corrections(&model_dir.join("corrections.txt"));
        // 合并自动学习的纠正
        let auto = load_corrections(&model_dir.join("auto_corrections.txt"));
        let auto_count = auto.len();
        corrections.extend(auto);

        let homophones = load_homophones(&model_dir.join("homophones.txt"));
        let bigram = BigramTable::load(&model_dir.join("bigram.bin"));

        log::info!(
            "纠错引擎: corrections {} 条 (自动 {}) | homophones {} 组 | bigram {} 条",
            corrections.len(), auto_count, homophones.len(), bigram.len()
        );

        Self { corrections, homophones, bigram, model_dir: model_dir.to_path_buf() }
    }

    /// 对整句做纠错
    pub fn correct(&self, text: &str) -> String {
        let text = self.apply_corrections(text);
        self.bigram_correct(&text)
    }

    /// 记录用户输出（更新 bigram）
    pub fn record(&mut self, text: &str) {
        self.bigram.record_text(text);
    }

    pub fn flush(&mut self) {
        self.bigram.save();
    }

    // ═══ 自学习 ═══

    /// 从用户纠正中学习
    pub fn learn_from_correction(&mut self, original: &str, corrected: &str) {
        let diffs = find_diffs(original, corrected);
        if diffs.is_empty() { return; }

        for (wrong, right) in &diffs {
            // 1. 追加到 auto_corrections.txt
            self.append_auto_correction(wrong, right);

            // 2. 更新内存中的 corrections
            self.corrections.insert(wrong.clone(), right.clone());

            // 3. bigram 频率调整
            self.bigram.adjust_text(wrong, -10);
            self.bigram.adjust_text(right, 10);

            // 4. 追加热词
            self.append_hotword(right);

            log::info!("📚 学习: {} → {}", wrong, right);
        }
    }

    fn append_auto_correction(&self, wrong: &str, right: &str) {
        let path = self.model_dir.join("auto_corrections.txt");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true).append(true).open(&path)
        {
            let _ = writeln!(f, "{}→{}", wrong, right);
        }
    }

    fn append_hotword(&self, word: &str) {
        let path = self.model_dir.join("hotwords.txt");
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        // 检查是否已存在（忽略权重后缀）
        let exists = content.lines().any(|line| {
            let w = line.split_whitespace().next().unwrap_or("");
            w == word
        });
        if !exists {
            if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(&path) {
                let _ = writeln!(f, "{} 5.0", word);
                log::info!("📝 热词追加: {}", word);
            }
        }
    }

    // ═══ 内部纠错 ═══

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
            if !is_cjk(c1) || !is_cjk(c2) { continue; }

            let current_freq = self.bigram.freq(c1, c2);
            if current_freq > 500 { continue; }

            // 尝试替换 c2
            if let Some(candidates) = self.homophones.get(&c2) {
                let mut best = (c2, current_freq);
                for &alt in candidates {
                    if alt == c2 { continue; }
                    let f = self.bigram.freq(c1, alt);
                    if f > best.1.max(1) * 10 { best = (alt, f); }
                }
                if best.0 != c2 { result[i + 1] = best.0; }
            }

            // 尝试替换 c1
            if result[i + 1] == c2 {
                if let Some(candidates) = self.homophones.get(&c1) {
                    let mut best = (c1, self.bigram.freq(c1, c2));
                    for &alt in candidates {
                        if alt == c1 { continue; }
                        let f = self.bigram.freq(alt, c2);
                        if f > best.1.max(1) * 10 { best = (alt, f); }
                    }
                    if best.0 != c1 { result[i] = best.0; }
                }
            }
        }
        result.iter().collect()
    }
}

/// 对比两个字符串，找出被替换的片段
pub fn find_diffs(original: &str, corrected: &str) -> Vec<(String, String)> {
    let orig: Vec<char> = original.chars().collect();
    let corr: Vec<char> = corrected.chars().collect();
    let mut diffs = Vec::new();
    let mut i = 0;
    let mut j = 0;

    while i < orig.len() && j < corr.len() {
        if orig[i] == corr[j] {
            i += 1;
            j += 1;
        } else {
            let si = i;
            let sj = j;
            let mut found = false;
            for di in 1..10.min(orig.len() - i + 1) {
                for dj in 1..10.min(corr.len() - j + 1) {
                    if i + di < orig.len() && j + dj < corr.len()
                        && orig[i + di] == corr[j + dj]
                    {
                        let wrong: String = orig[si..i + di].iter().collect();
                        let right: String = corr[sj..j + dj].iter().collect();
                        if !wrong.is_empty() && !right.is_empty() && wrong != right {
                            diffs.push((wrong, right));
                        }
                        i += di;
                        j += dj;
                        found = true;
                        break;
                    }
                }
                if found { break; }
            }
            if !found {
                // 尾部不同
                let wrong: String = orig[si..].iter().collect();
                let right: String = corr[sj..].iter().collect();
                if !wrong.is_empty() && !right.is_empty() && wrong != right {
                    diffs.push((wrong, right));
                }
                break;
            }
        }
    }
    diffs
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
        let parts: Vec<&str> = if line.contains('\u{2192}') { // →
            line.splitn(2, '\u{2192}').collect()
        } else if line.contains("=>") {
            line.splitn(2, "=>").collect()
        } else { continue };
        if parts.len() == 2 {
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
        for &c in &chars {
            let others: Vec<char> = chars.iter().filter(|&&x| x != c).copied().collect();
            map.entry(c).or_default().extend(others);
        }
    }
    for v in map.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_diffs_single() {
        let diffs = find_diffs("我想看看边城的效果", "我想看看编程的效果");
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0], ("边城".to_string(), "编程".to_string()));
    }

    #[test]
    fn test_find_diffs_multiple() {
        let diffs = find_diffs("他的工做很好我门", "他的工作很好我们");
        assert!(diffs.iter().any(|(w, _)| w == "做"));
        assert!(diffs.iter().any(|(w, _)| w == "门"));
    }

    #[test]
    fn test_find_diffs_identical() {
        let diffs = find_diffs("完全相同", "完全相同");
        assert!(diffs.is_empty());
    }
}