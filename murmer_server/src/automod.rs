//! Auto-moderation rules: operator-configured patterns, each carrying the
//! action the server takes on a message that matches it.
//!
//! This is the general form of the flat word list in [`crate::profanity`],
//! which stays what it is — words to mask. A rule here matches by whole word,
//! by substring or by regular expression, and can answer with something a
//! mask cannot: refusing the message, or muting whoever sent it.
//!
//! Three decisions shape the rest of the file:
//!
//! - **Rules are compiled once**, into a [`RuleSet`] held in
//!   `AppState.automod`. Every chat message and every edit is checked against
//!   every rule, so compiling a regex per message would put the cost of the
//!   feature on the hot path instead of on the save.
//! - **The most severe match wins**, not the first one. An operator who
//!   writes an overlapping pair of rules means the harsher one; making them
//!   order the list by hand to get that would be a trap, because the pair
//!   that overlaps is usually the pair they never noticed overlaps.
//! - **A mute rule always has a duration.** A manual mute may be indefinite
//!   because a moderator chose it and can lift it; an indefinite mute handed
//!   out by a mistyped pattern, with nobody watching, is a different thing.
//!
//! Regular expressions come from the `regex` crate, which does not backtrack
//! and so has no catastrophic case: a pattern an operator pastes in cannot
//! cost more than linear time in the length of the message. What is left to
//! bound is compile-time memory, hence [`REGEX_SIZE_LIMIT`].

use regex::{Regex, RegexBuilder};
use tracing::warn;

use crate::profanity::word_runs;
// The same bounds a moderator's mute is held to; a rule may not reach past
// what a person could do by hand.
use crate::db::{MAX_MUTE_SECONDS, MIN_MUTE_SECONDS};

/// Maximum number of rules a server may hold.
pub const MAX_AUTOMOD_RULES: usize = 50;

/// Maximum length in bytes of a rule's pattern.
pub const MAX_AUTOMOD_PATTERN_LEN: usize = 200;

/// Maximum length in bytes of a rule's name.
pub const MAX_AUTOMOD_NAME_LEN: usize = 48;

/// Mute length a rule starts with when none is given, in seconds.
pub const DEFAULT_AUTOMOD_MUTE_SECONDS: i64 = 300;

/// Compiled-program budget for one regular expression. The matcher is
/// linear-time whatever the pattern, so this bounds memory rather than time —
/// it is what stops a pasted monster of a pattern from costing megabytes of
/// resident state per rule.
const REGEX_SIZE_LIMIT: usize = 64 * 1024;

/// How a rule's pattern is compared against a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleKind {
    /// One whole word, case-insensitively — the profanity filter's matcher,
    /// so a rule for "ass" leaves "class" alone.
    Word,
    /// Anywhere in the text, case-insensitively.
    Substring,
    /// A regular expression, case-insensitively.
    Regex,
}

impl RuleKind {
    /// The name this kind travels under, on the wire and in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Word => "word",
            Self::Substring => "substring",
            Self::Regex => "regex",
        }
    }

    /// Parse a wire name; `None` for anything this build does not know.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "word" => Some(Self::Word),
            "substring" => Some(Self::Substring),
            "regex" => Some(Self::Regex),
            _ => None,
        }
    }
}

/// What the server does with a message that matched a rule.
///
/// The variants are ordered from least to most severe and compared as such:
/// when several rules match one message, the greatest wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuleAction {
    /// Deliver the message and tell the sender privately that it matched.
    /// The action to give a rule whose pattern is still being tuned: one that
    /// turns out to match half the room has warned people rather than
    /// silenced them.
    Warn,
    /// Refuse the message. It is never stored and never broadcast, so unlike
    /// a moderator's deletion there is no window in which anyone saw it.
    Delete,
    /// Refuse the message and mute the sender for the rule's duration.
    Mute,
}

impl RuleAction {
    /// The name this action travels under, on the wire and in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warn => "warn",
            Self::Delete => "delete",
            Self::Mute => "mute",
        }
    }

    /// Parse a wire name; `None` for anything this build does not know.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "warn" => Some(Self::Warn),
            "delete" => Some(Self::Delete),
            "mute" => Some(Self::Mute),
            _ => None,
        }
    }
}

/// One auto-moderation rule as an operator configured it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutomodRule {
    /// Operator's label for the rule. It is the only part of a rule ever
    /// disclosed to the person who tripped it, so it may be empty — the
    /// pattern itself never leaves the dashboard.
    pub name: String,
    pub pattern: String,
    pub kind: RuleKind,
    pub action: RuleAction,
    /// Mute length in seconds; only consulted for [`RuleAction::Mute`].
    pub mute_seconds: i64,
    /// A disabled rule is kept but never matched, so an operator can retire a
    /// pattern without losing it.
    pub enabled: bool,
}

/// Why a rule was refused. Rules are rejected rather than quietly dropped: a
/// pattern that fails to compile is a mistake its author has to see, and a
/// dashboard that silently saves fewer rules than it was given is the worst
/// version of that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleError {
    TooManyRules,
    NameTooLong,
    EmptyPattern,
    PatternTooLong,
    /// A `word` rule whose pattern is not a single word, and which therefore
    /// could never match anything.
    PatternNotAWord,
    InvalidRegex,
}

/// Trim a rule and clamp its mute duration into this build's bounds. Applied
/// on write *and* on read, so a row written by another build — or by hand —
/// can never hand the matcher something unbounded.
pub fn normalize_rule(rule: &AutomodRule) -> AutomodRule {
    AutomodRule {
        name: rule.name.trim().to_string(),
        pattern: rule.pattern.trim().to_string(),
        kind: rule.kind,
        action: rule.action,
        mute_seconds: rule.mute_seconds.clamp(MIN_MUTE_SECONDS, MAX_MUTE_SECONDS),
        enabled: rule.enabled,
    }
}

/// Check a normalized rule against this build's bounds.
pub fn validate_rule(rule: &AutomodRule) -> Result<(), RuleError> {
    if rule.name.len() > MAX_AUTOMOD_NAME_LEN {
        return Err(RuleError::NameTooLong);
    }
    if rule.pattern.is_empty() {
        return Err(RuleError::EmptyPattern);
    }
    if rule.pattern.len() > MAX_AUTOMOD_PATTERN_LEN {
        return Err(RuleError::PatternTooLong);
    }
    match rule.kind {
        // A word rule is compared against single word runs, so a pattern with
        // a space in it can never match. Refusing it is the difference
        // between a rule reported broken and one that silently never fires.
        RuleKind::Word if rule.pattern.chars().any(char::is_whitespace) => {
            Err(RuleError::PatternNotAWord)
        }
        RuleKind::Regex => build_regex(&rule.pattern)
            .map(|_| ())
            .map_err(|_| RuleError::InvalidRegex),
        _ => Ok(()),
    }
}

/// Normalize and validate a whole list, as it arrives from a dashboard save.
/// Returns the normalized rules, or the first problem found.
pub fn normalize_rules(rules: &[AutomodRule]) -> Result<Vec<AutomodRule>, RuleError> {
    if rules.len() > MAX_AUTOMOD_RULES {
        return Err(RuleError::TooManyRules);
    }
    let mut normalized = Vec::with_capacity(rules.len());
    for rule in rules {
        let rule = normalize_rule(rule);
        validate_rule(&rule)?;
        normalized.push(rule);
    }
    Ok(normalized)
}

/// Compile a case-insensitive pattern under the memory budget.
fn build_regex(pattern: &str) -> Result<Regex, regex::Error> {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .size_limit(REGEX_SIZE_LIMIT)
        .build()
}

/// A rule with its matcher already built.
struct CompiledRule {
    rule: AutomodRule,
    matcher: Matcher,
}

enum Matcher {
    /// Lowercased, compared against each word run of the message.
    Word(String),
    /// Lowercased, searched for in the lowercased message.
    Substring(String),
    Regex(Regex),
}

impl CompiledRule {
    fn compile(rule: AutomodRule) -> Result<Self, RuleError> {
        let matcher = match rule.kind {
            RuleKind::Word => Matcher::Word(rule.pattern.to_lowercase()),
            RuleKind::Substring => Matcher::Substring(rule.pattern.to_lowercase()),
            RuleKind::Regex => {
                Matcher::Regex(build_regex(&rule.pattern).map_err(|_| RuleError::InvalidRegex)?)
            }
        };
        Ok(Self { rule, matcher })
    }

    fn matches(&self, text: &str) -> bool {
        match &self.matcher {
            Matcher::Word(word) => word_runs(text).any(|run| run.to_lowercase() == *word),
            Matcher::Substring(needle) => text.to_lowercase().contains(needle),
            Matcher::Regex(regex) => regex.is_match(text),
        }
    }
}

/// The compiled rules, in the order the operator arranged them.
#[derive(Default)]
pub struct RuleSet {
    rules: Vec<CompiledRule>,
}

impl RuleSet {
    /// Compile a stored list. A rule that no longer compiles is dropped with a
    /// log line rather than taking the rest of the list down with it: the list
    /// is written validated, so reaching this means the row was edited by
    /// something other than this build, and the other rules are still
    /// somebody's moderation policy.
    pub fn compile(rules: Vec<AutomodRule>) -> Self {
        let mut compiled = Vec::with_capacity(rules.len());
        for rule in rules {
            match CompiledRule::compile(rule) {
                Ok(rule) => compiled.push(rule),
                Err(error) => warn!(?error, "Skipping unusable auto-moderation rule"),
            }
        }
        Self { rules: compiled }
    }

    /// Whether any rule could match at all. Checked first on the message
    /// path, because the common case is a server with no rules configured.
    pub fn is_active(&self) -> bool {
        self.rules.iter().any(|entry| entry.rule.enabled)
    }

    /// The most severe rule matching `text`, or `None`.
    ///
    /// Ties go to the rule the operator put first, which is why this reaches
    /// for the *minimum* of the reversed severity: `max_by_key` returns the
    /// last of several equal maxima, `min_by_key` the first of equal minima.
    pub fn evaluate(&self, text: &str) -> Option<&AutomodRule> {
        self.rules
            .iter()
            .filter(|entry| entry.rule.enabled && entry.matches(text))
            .min_by_key(|entry| std::cmp::Reverse(entry.rule.action))
            .map(|entry| &entry.rule)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(pattern: &str, kind: RuleKind, action: RuleAction) -> AutomodRule {
        AutomodRule {
            name: format!("rule {pattern}"),
            pattern: pattern.to_string(),
            kind,
            action,
            mute_seconds: DEFAULT_AUTOMOD_MUTE_SECONDS,
            enabled: true,
        }
    }

    #[test]
    fn word_rules_match_whole_words_only() {
        let set = RuleSet::compile(vec![rule("ass", RuleKind::Word, RuleAction::Delete)]);
        assert!(set.evaluate("ass").is_some());
        assert!(set.evaluate("Well, ASS!").is_some());
        assert!(set.evaluate("class pass").is_none());
    }

    #[test]
    fn substring_rules_match_inside_words() {
        let set = RuleSet::compile(vec![rule(
            "discord.gg/",
            RuleKind::Substring,
            RuleAction::Delete,
        )]);
        assert!(set.evaluate("join https://DISCORD.GG/abcd now").is_some());
        assert!(set.evaluate("nothing to see").is_none());
    }

    #[test]
    fn regex_rules_match_patterns() {
        let set = RuleSet::compile(vec![rule(
            r"\b\d{4}[- ]?\d{4}\b",
            RuleKind::Regex,
            RuleAction::Delete,
        )]);
        assert!(set.evaluate("code 1234 5678").is_some());
        assert!(set.evaluate("code 12 34").is_none());
    }

    #[test]
    fn the_most_severe_match_wins() {
        let set = RuleSet::compile(vec![
            rule("spam", RuleKind::Word, RuleAction::Warn),
            rule("spam", RuleKind::Substring, RuleAction::Mute),
            rule("spam", RuleKind::Substring, RuleAction::Delete),
        ]);
        assert_eq!(
            set.evaluate("spam").map(|found| found.action),
            Some(RuleAction::Mute)
        );
    }

    #[test]
    fn equally_severe_matches_resolve_to_the_first() {
        let set = RuleSet::compile(vec![
            rule("first", RuleKind::Substring, RuleAction::Delete),
            rule("second", RuleKind::Substring, RuleAction::Delete),
        ]);
        assert_eq!(
            set.evaluate("first second")
                .map(|found| found.pattern.as_str()),
            Some("first")
        );
    }

    #[test]
    fn disabled_rules_never_match() {
        let mut disabled = rule("spam", RuleKind::Substring, RuleAction::Delete);
        disabled.enabled = false;
        let set = RuleSet::compile(vec![disabled]);
        assert!(!set.is_active());
        assert!(set.evaluate("spam").is_none());
    }

    #[test]
    fn invalid_rules_are_refused_rather_than_dropped() {
        assert_eq!(
            validate_rule(&rule("(unclosed", RuleKind::Regex, RuleAction::Warn)),
            Err(RuleError::InvalidRegex)
        );
        assert_eq!(
            validate_rule(&rule("two words", RuleKind::Word, RuleAction::Warn)),
            Err(RuleError::PatternNotAWord)
        );
        assert_eq!(
            validate_rule(&rule("", RuleKind::Substring, RuleAction::Warn)),
            Err(RuleError::EmptyPattern)
        );
        let mut oversized = rule("x", RuleKind::Substring, RuleAction::Warn);
        oversized.pattern = "x".repeat(MAX_AUTOMOD_PATTERN_LEN + 1);
        assert_eq!(validate_rule(&oversized), Err(RuleError::PatternTooLong));
        oversized.pattern = "x".into();
        oversized.name = "n".repeat(MAX_AUTOMOD_NAME_LEN + 1);
        assert_eq!(validate_rule(&oversized), Err(RuleError::NameTooLong));

        let many = vec![rule("x", RuleKind::Substring, RuleAction::Warn); MAX_AUTOMOD_RULES + 1];
        assert_eq!(normalize_rules(&many), Err(RuleError::TooManyRules));
    }

    #[test]
    fn normalization_trims_and_clamps() {
        let mut raw = rule("  spam  ", RuleKind::Substring, RuleAction::Mute);
        raw.name = "  Ads  ".into();
        raw.mute_seconds = MAX_MUTE_SECONDS * 10;
        let normalized = normalize_rules(std::slice::from_ref(&raw)).expect("valid");
        assert_eq!(normalized[0].name, "Ads");
        assert_eq!(normalized[0].pattern, "spam");
        assert_eq!(normalized[0].mute_seconds, MAX_MUTE_SECONDS);

        raw.mute_seconds = 0;
        let normalized = normalize_rules(std::slice::from_ref(&raw)).expect("valid");
        assert_eq!(normalized[0].mute_seconds, MIN_MUTE_SECONDS);
    }

    #[test]
    fn wire_names_round_trip() {
        for kind in [RuleKind::Word, RuleKind::Substring, RuleKind::Regex] {
            assert_eq!(RuleKind::parse(kind.as_str()), Some(kind));
        }
        for action in [RuleAction::Warn, RuleAction::Delete, RuleAction::Mute] {
            assert_eq!(RuleAction::parse(action.as_str()), Some(action));
        }
        assert_eq!(RuleKind::parse("nonsense"), None);
        assert_eq!(RuleAction::parse("ban"), None);
    }
}
