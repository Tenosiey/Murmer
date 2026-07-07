use murmer_server::ws::helpers::reply_preview;

#[test]
fn short_text_is_untouched() {
    assert_eq!(reply_preview("hello", 200), "hello");
    assert_eq!(reply_preview("", 200), "");
}

#[test]
fn long_text_is_truncated_to_the_limit() {
    let text = "a".repeat(300);
    let preview = reply_preview(&text, 200);
    assert_eq!(preview.chars().count(), 200);
}

#[test]
fn truncation_respects_multibyte_characters() {
    // Each emoji is one char but four bytes; a byte-based cut would panic or
    // produce invalid UTF-8.
    let text = "🦀".repeat(50);
    let preview = reply_preview(&text, 10);
    assert_eq!(preview.chars().count(), 10);
    assert!(preview.chars().all(|c| c == '🦀'));
}
