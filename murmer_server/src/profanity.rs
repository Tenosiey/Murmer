//! The server-side profanity filter.
//!
//! The word list is configured in the Server Dashboard and applied to every
//! chat message before it is stored or broadcast, so a masked word never
//! exists anywhere a client could read it back.
//!
//! Matching is per *word*, not per substring: the text is walked as runs of
//! alphanumeric characters and each run is compared, lowercased, against the
//! list. That is what keeps "class" out of a filter for "ass" — a substring
//! match would mangle innocent words and quickly train operators to turn the
//! feature off. It also means the mask keeps the message's shape (one `*` per
//! character) instead of leaking the length of the list entry.

/// Character a filtered word is replaced with.
const MASK_CHAR: char = '*';

/// Whether `c` is part of a word for matching purposes.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
}

/// Replace every word of `text` that appears in `words` with asterisks.
/// Returns `None` when nothing matched, so the common case allocates nothing
/// and the caller can leave the message untouched.
///
/// `words` is expected to be lowercase and trimmed
/// (`db::normalize_profanity_words` guarantees that on read and on write).
pub fn mask(text: &str, words: &[String]) -> Option<String> {
    if words.is_empty() {
        return None;
    }

    let mut out = String::with_capacity(text.len());
    let mut matched = false;
    let mut word = String::new();

    // Flush the pending word run, masked if it is on the list.
    let flush = |word: &mut String, out: &mut String, matched: &mut bool| {
        if word.is_empty() {
            return;
        }
        let lowered = word.to_lowercase();
        if words.iter().any(|entry| entry == &lowered) {
            for _ in word.chars() {
                out.push(MASK_CHAR);
            }
            *matched = true;
        } else {
            out.push_str(word);
        }
        word.clear();
    };

    for c in text.chars() {
        if is_word_char(c) {
            word.push(c);
        } else {
            flush(&mut word, &mut out, &mut matched);
            out.push(c);
        }
    }
    flush(&mut word, &mut out, &mut matched);

    matched.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn masks_whole_words_case_insensitively() {
        let list = words(&["damn", "heck"]);
        assert_eq!(
            mask("Damn, that HECK", &list).as_deref(),
            Some("****, that ****")
        );
        assert_eq!(mask("well, damn!", &list).as_deref(), Some("well, ****!"));
    }

    #[test]
    fn leaves_innocent_words_alone() {
        let list = words(&["ass"]);
        // A substring matcher would turn these into "cl***" and "p***".
        assert_eq!(mask("class pass", &list), None);
        assert_eq!(mask("ass", &list).as_deref(), Some("***"));
    }

    #[test]
    fn returns_none_when_nothing_matches() {
        assert_eq!(mask("hello there", &words(&["damn"])), None);
        assert_eq!(mask("anything at all", &[]), None);
    }

    #[test]
    fn preserves_surrounding_punctuation_and_unicode() {
        let list = words(&["mist"]);
        assert_eq!(
            mask("(mist) — mist, mistig", &list).as_deref(),
            Some("(****) — ****, mistig")
        );
        // Non-ASCII words are matched by their lowercase form, and the mask
        // keeps one character per character rather than per byte.
        let list = words(&["straße"]);
        assert_eq!(mask("Straße!", &list).as_deref(), Some("******!"));
    }
}
