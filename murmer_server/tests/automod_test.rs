//! Tests for the auto-moderation rules: persistence, the bounds a stored row
//! is held to, and what a row this build cannot use does to the rest.
//!
//! The matcher itself is covered by the unit tests in `src/automod.rs`. What
//! is worth an integration test is the round trip, because every failure mode
//! here is silent: a rule that comes back with a different action moderates
//! something the operator did not ask for, a rule dropped on load stops
//! moderating without saying so, and a mute duration that survives
//! unclamped is a punishment nobody chose. None of it shows up until the
//! rule fires on somebody.

use murmer_server::automod::{
    AutomodRule, DEFAULT_AUTOMOD_MUTE_SECONDS, MAX_AUTOMOD_RULES, RuleAction, RuleKind, RuleSet,
};
use murmer_server::db::{self, DbCall, MAX_MUTE_SECONDS, MIN_MUTE_SECONDS};

async fn setup() -> db::Db {
    db::init(":memory:").await.expect("in-memory db")
}

fn rule(name: &str, pattern: &str, kind: RuleKind, action: RuleAction) -> AutomodRule {
    AutomodRule {
        name: name.to_string(),
        pattern: pattern.to_string(),
        kind,
        action,
        mute_seconds: DEFAULT_AUTOMOD_MUTE_SECONDS,
        enabled: true,
    }
}

#[tokio::test]
async fn a_fresh_server_moderates_nothing() {
    let db = setup().await;

    let rules = db::automod_rules(&db).await.expect("load rules");
    assert!(rules.is_empty());
    assert!(!RuleSet::compile(rules).is_active());
}

#[tokio::test]
async fn rules_round_trip_in_the_operators_order() {
    let db = setup().await;

    let stored = vec![
        rule(
            "Invites",
            "discord.gg/",
            RuleKind::Substring,
            RuleAction::Delete,
        ),
        rule("Slurs", "heck", RuleKind::Word, RuleAction::Mute),
        rule("Shouting", r"[A-Z]{20,}", RuleKind::Regex, RuleAction::Warn),
    ];
    let written = db::set_automod_rules(&db, &stored).await.expect("store");
    assert_eq!(written, stored);

    let loaded = db::automod_rules(&db).await.expect("load");
    assert_eq!(loaded, stored);
    // Order is the operator's, and it decides which of two equally severe
    // rules a message is reported against.
    assert_eq!(
        loaded.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        ["Invites", "Slurs", "Shouting"]
    );
}

#[tokio::test]
async fn saving_replaces_the_whole_list() {
    let db = setup().await;

    db::set_automod_rules(
        &db,
        &[
            rule("One", "one", RuleKind::Word, RuleAction::Warn),
            rule("Two", "two", RuleKind::Word, RuleAction::Warn),
        ],
    )
    .await
    .expect("store");

    // A save is the new list, not an addition to the old one — otherwise a
    // rule an operator deleted would keep moderating.
    db::set_automod_rules(
        &db,
        &[rule("Three", "three", RuleKind::Word, RuleAction::Warn)],
    )
    .await
    .expect("store");
    let loaded = db::automod_rules(&db).await.expect("load");
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "Three");

    db::set_automod_rules(&db, &[]).await.expect("store");
    assert!(db::automod_rules(&db).await.expect("load").is_empty());
}

#[tokio::test]
async fn stored_mute_durations_are_clamped_on_read() {
    let db = setup().await;

    let mut long = rule("Ads", "buy now", RuleKind::Substring, RuleAction::Mute);
    long.mute_seconds = MAX_MUTE_SECONDS * 100;
    let mut short = rule("Links", "http", RuleKind::Substring, RuleAction::Mute);
    short.mute_seconds = 0;
    db::set_automod_rules(&db, &[long, short])
        .await
        .expect("store");

    // Clamping happens on write, but the assertion that matters is the read:
    // a row edited by hand, or written by another build, must not be able to
    // hand the mute path a duration this build would never issue.
    let loaded = db::automod_rules(&db).await.expect("load");
    assert_eq!(loaded[0].mute_seconds, MAX_MUTE_SECONDS);
    assert_eq!(loaded[1].mute_seconds, MIN_MUTE_SECONDS);
}

#[tokio::test]
async fn rows_of_an_unknown_type_are_skipped_not_guessed() {
    let db = setup().await;

    db::set_automod_rules(
        &db,
        &[
            rule("Known", "spam", RuleKind::Word, RuleAction::Delete),
            rule("Also known", "ads", RuleKind::Word, RuleAction::Warn),
        ],
    )
    .await
    .expect("store");

    // Stand in for a row written by a build that knows an action this one does
    // not. Falling back to some default would apply an action the operator
    // never chose, so the row is dropped — and the rules around it survive.
    db.call_db(|conn| {
        conn.execute(
            "UPDATE automod_rules SET action = 'kick' WHERE position = 0",
            [],
        )
    })
    .await
    .expect("rewrite row");

    let loaded = db::automod_rules(&db).await.expect("load");
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "Also known");
}

#[tokio::test]
async fn a_pattern_that_stopped_compiling_does_not_take_the_list_with_it() {
    let db = setup().await;

    db::set_automod_rules(
        &db,
        &[
            rule("Broken", r"\d+", RuleKind::Regex, RuleAction::Delete),
            rule("Fine", "spam", RuleKind::Word, RuleAction::Delete),
        ],
    )
    .await
    .expect("store");

    db.call_db(|conn| {
        conn.execute(
            "UPDATE automod_rules SET pattern = '(unclosed' WHERE position = 0",
            [],
        )
    })
    .await
    .expect("rewrite row");

    // The rules are written validated, so an uncompilable pattern means the
    // row came from somewhere else. The rest of the list is still somebody's
    // moderation policy and keeps working.
    let set = RuleSet::compile(db::automod_rules(&db).await.expect("load"));
    assert!(set.evaluate("spam").is_some());
    assert!(set.evaluate("1234").is_none());
}

#[tokio::test]
async fn the_rule_ceiling_is_storable_end_to_end() {
    let db = setup().await;

    let full: Vec<AutomodRule> = (0..MAX_AUTOMOD_RULES)
        .map(|i| {
            rule(
                &format!("rule {i}"),
                &format!("word{i}"),
                RuleKind::Word,
                RuleAction::Warn,
            )
        })
        .collect();
    db::set_automod_rules(&db, &full).await.expect("store");
    assert_eq!(
        db::automod_rules(&db).await.expect("load").len(),
        MAX_AUTOMOD_RULES
    );
}
