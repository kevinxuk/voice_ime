// src/punctuation.rs — 规则标点

/// 给无标点文本追加句末标点
pub fn add_punctuation(text: &str) -> String {
    if text.is_empty() { return text.to_string(); }

    let trimmed = text.trim();
    if trimmed.is_empty() { return text.to_string(); }

    let last = trimmed.chars().last().unwrap();

    // 已有标点不重复加
    if "。？！，、；：…".contains(last) {
        return trimmed.to_string();
    }

    let mut result = trimmed.to_string();

    // 问句语气词
    if "吗呢吧嘛么".contains(last) {
        result.push('？');
    }
    // 感叹语气词
    else if "啊哇呀哦耶".contains(last) {
        result.push('！');
    }
    // 默认句号
    else {
        result.push('。');
    }

    result
}