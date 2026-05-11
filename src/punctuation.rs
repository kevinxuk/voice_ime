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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_period() {
        assert_eq!(add_punctuation("你好世界"), "你好世界。");
    }

    #[test]
    fn test_question() {
        assert_eq!(add_punctuation("你好吗"), "你好吗？");
        assert_eq!(add_punctuation("是不是呢"), "是不是呢？");
    }

    #[test]
    fn test_exclamation() {
        assert_eq!(add_punctuation("太好了啊"), "太好了啊！");
    }

    #[test]
    fn test_existing_punctuation() {
        assert_eq!(add_punctuation("你好。"), "你好。");
        assert_eq!(add_punctuation("是吗？"), "是吗？");
    }

    #[test]
    fn test_empty() {
        assert_eq!(add_punctuation(""), "");
        assert_eq!(add_punctuation("  "), "  ");
    }
}