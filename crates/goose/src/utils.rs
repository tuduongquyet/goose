use tokio_util::sync::CancellationToken;
use unicode_normalization::UnicodeNormalization;

/// Check if a character is in the Unicode Tags Block range (U+E0000-U+E007F)
/// These characters are invisible and can be used for steganographic attacks
fn is_in_unicode_tag_range(c: char) -> bool {
    matches!(c, '\u{E0000}'..='\u{E007F}')
}

pub fn contains_unicode_tags(text: &str) -> bool {
    text.chars().any(is_in_unicode_tag_range)
}

/// Sanitize Unicode Tags Block characters from text
pub fn sanitize_unicode_tags(text: &str) -> String {
    let normalized: String = text.nfc().collect();

    normalized
        .chars()
        .filter(|&c| !is_in_unicode_tag_range(c))
        .collect()
}

/// Safely truncate a string at character boundaries, not byte boundaries
///
/// This function ensures that multi-byte UTF-8 characters (like Japanese, emoji, etc.)
/// are not split in the middle, which would cause a panic.
///
/// # Arguments
/// * `s` - The string to truncate
/// * `max_chars` - Maximum number of characters to keep
///
/// # Returns
/// A truncated string with "..." appended if truncation occurred
pub fn safe_truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

/// Truncate large text keeping the head and tail, dropping the middle.
///
/// This preserves the beginning (setup, headers, context) and end (final output,
/// errors, conclusions) of large outputs while dropping the noisy middle.
///
/// # Arguments
/// * `s` - The string to truncate
/// * `max_chars` - Maximum character count for the result (excluding the truncation notice)
/// * `head_ratio` - Fraction of max_chars to allocate to the head (e.g., 0.4 = 40% head, 60% tail)
pub fn head_tail_truncate(s: &str, max_chars: usize, head_ratio: f64) -> String {
    let total_chars = s.chars().count();
    if total_chars <= max_chars {
        return s.to_string();
    }

    let head_ratio = head_ratio.clamp(0.0, 1.0);
    let head_chars = ((max_chars as f64) * head_ratio) as usize;
    let tail_chars = max_chars.saturating_sub(head_chars);
    let dropped = total_chars.saturating_sub(head_chars + tail_chars);

    let head: String = s.chars().take(head_chars).collect();
    let tail: String = s
        .chars()
        .skip(total_chars.saturating_sub(tail_chars))
        .collect();

    format!(
        "{}\n\n... [{} characters truncated] ...\n\n{}",
        head, dropped, tail
    )
}

pub fn is_token_cancelled(cancellation_token: &Option<CancellationToken>) -> bool {
    cancellation_token
        .as_ref()
        .is_some_and(|t| t.is_cancelled())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_unicode_tags() {
        // Test detection of Unicode Tags Block characters
        assert!(contains_unicode_tags("Hello\u{E0041}world"));
        assert!(contains_unicode_tags("\u{E0000}"));
        assert!(contains_unicode_tags("\u{E007F}"));
        assert!(!contains_unicode_tags("Hello world"));
        assert!(!contains_unicode_tags("Hello 世界 🌍"));
        assert!(!contains_unicode_tags(""));
    }

    #[test]
    fn test_sanitize_unicode_tags() {
        // Test that Unicode Tags Block characters are removed
        let malicious = "Hello\u{E0041}\u{E0042}\u{E0043}world"; // Invisible "ABC"
        let cleaned = sanitize_unicode_tags(malicious);
        assert_eq!(cleaned, "Helloworld");
    }

    #[test]
    fn test_sanitize_unicode_tags_preserves_legitimate_unicode() {
        // Test that legitimate Unicode characters are preserved
        let clean_text = "Hello world 世界 🌍";
        let cleaned = sanitize_unicode_tags(clean_text);
        assert_eq!(cleaned, clean_text);
    }

    #[test]
    fn test_sanitize_unicode_tags_empty_string() {
        let empty = "";
        let cleaned = sanitize_unicode_tags(empty);
        assert_eq!(cleaned, "");
    }

    #[test]
    fn test_sanitize_unicode_tags_only_malicious() {
        // Test string containing only Unicode Tags characters
        let only_malicious = "\u{E0041}\u{E0042}\u{E0043}";
        let cleaned = sanitize_unicode_tags(only_malicious);
        assert_eq!(cleaned, "");
    }

    #[test]
    fn test_sanitize_unicode_tags_mixed_content() {
        // Test mixed legitimate and malicious Unicode
        let mixed = "Hello\u{E0041} 世界\u{E0042} 🌍\u{E0043}!";
        let cleaned = sanitize_unicode_tags(mixed);
        assert_eq!(cleaned, "Hello 世界 🌍!");
    }

    #[test]
    fn test_safe_truncate_ascii() {
        assert_eq!(safe_truncate("hello world", 20), "hello world");
        assert_eq!(safe_truncate("hello world", 8), "hello...");
        assert_eq!(safe_truncate("hello", 5), "hello");
        assert_eq!(safe_truncate("hello", 3), "...");
    }

    #[test]
    fn test_safe_truncate_japanese() {
        // Japanese characters: "こんにちは世界" (Hello World)
        let japanese = "こんにちは世界";
        assert_eq!(safe_truncate(japanese, 10), japanese);
        assert_eq!(safe_truncate(japanese, 5), "こん...");
        assert_eq!(safe_truncate(japanese, 7), japanese);
    }

    #[test]
    fn test_safe_truncate_mixed() {
        // Mixed ASCII and Japanese
        let mixed = "Hello こんにちは";
        assert_eq!(safe_truncate(mixed, 20), mixed);
        assert_eq!(safe_truncate(mixed, 8), "Hello...");
    }

    #[test]
    fn test_head_tail_truncate_short_passthrough() {
        assert_eq!(head_tail_truncate("hello", 100, 0.4), "hello");
        assert_eq!(head_tail_truncate("", 100, 0.4), "");
    }

    #[test]
    fn test_head_tail_truncate_splits_correctly() {
        let input = "a".repeat(1000);
        let result = head_tail_truncate(&input, 100, 0.4);
        assert!(result.contains("characters truncated"));
        // Head should be 40 chars of 'a'
        assert!(result.starts_with(&"a".repeat(40)));
        // Tail should be 60 chars of 'a'
        assert!(result.ends_with(&"a".repeat(60)));
    }

    #[test]
    fn test_head_tail_truncate_preserves_content() {
        // Build a string where head and tail are distinguishable
        let head_part = "HEAD".repeat(50); // 200 chars
        let middle = "M".repeat(800);
        let tail_part = "TAIL".repeat(50); // 200 chars
        let input = format!("{}{}{}", head_part, middle, tail_part);
        assert_eq!(input.chars().count(), 1200);

        let result = head_tail_truncate(&input, 400, 0.5);
        // Head: 200 chars, Tail: 200 chars
        assert!(result.starts_with(&"HEAD".repeat(50)));
        assert!(result.ends_with(&"TAIL".repeat(50)));
        assert!(result.contains("800 characters truncated"));
    }

    #[test]
    fn test_head_tail_truncate_exact_boundary() {
        let input = "a".repeat(100);
        // Exactly at max — no truncation
        assert_eq!(head_tail_truncate(&input, 100, 0.4), input);
        // One over — triggers truncation
        let input101 = "a".repeat(101);
        let result = head_tail_truncate(&input101, 100, 0.4);
        assert!(result.contains("truncated"));
    }
}
