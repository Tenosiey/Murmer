//! Tests for server-issued invite codes.
//!
//! An invite is a credential on a password-protected server, so the rules
//! that decide whether one still opens the door are the whole feature: an
//! expiry that is not honoured, a use counter that can be raced past, or a
//! revoked code that keeps working all mean a leaked link is *not* in fact
//! recoverable — the exact problem invites were added to solve. None of that
//! is visible from the outside: a spent invite and a live one look identical
//! until somebody joins with it.
//!
//! The redemption path also has to leave a member's reconnects alone. That is
//! the assertion that pins the design decision behind `invite_members`:
//! membership outlives the invite, so revoking a code stops future joins
//! without evicting the people who already used it.

use chrono::{Duration, Utc};
use murmer_server::db::{self, Redemption};
use murmer_server::permissions::{CREATE_INVITES, DEFAULT_MOD, KICK_MEMBERS, VIEW_CHANNELS};

const KEY: &str = "key-alice";
const OTHER_KEY: &str = "key-bob";

/// A database with one invite in it, described by its expiry and use limit.
async fn setup(
    expires_in: Option<Duration>,
    max_uses: i64,
) -> (tokio_rusqlite::Connection, &'static str) {
    let database = db::init(":memory:").await.expect("in-memory db");
    let expires_at = expires_in.map(|d| Utc::now() + d);
    db::create_invite(&database, "code-1", "alice", expires_at, max_uses)
        .await
        .expect("create invite");
    (database, "code-1")
}

#[tokio::test]
async fn redeems_an_unlimited_invite_and_records_membership() {
    let (database, code) = setup(None, 0).await;

    assert!(!db::is_invite_member(&database, KEY).await.expect("member"));
    assert_eq!(
        db::redeem_invite(&database, code, KEY, Utc::now())
            .await
            .expect("redeem"),
        Redemption::Granted
    );
    assert!(db::is_invite_member(&database, KEY).await.expect("member"));

    let invite = &db::list_invites(&database).await.expect("list")[0];
    assert_eq!(invite.uses, 1);
    assert_eq!(invite.max_uses, 0);
    assert_eq!(invite.created_by, "alice");
    assert!(invite.expires_at.is_none());
}

#[tokio::test]
async fn refuses_an_unknown_code() {
    let (database, _) = setup(None, 0).await;
    assert_eq!(
        db::check_invite(&database, "not-a-code", Utc::now())
            .await
            .expect("check"),
        Redemption::Unknown
    );
    assert_eq!(
        db::redeem_invite(&database, "not-a-code", KEY, Utc::now())
            .await
            .expect("redeem"),
        Redemption::Unknown
    );
    assert!(!db::is_invite_member(&database, KEY).await.expect("member"));
}

#[tokio::test]
async fn refuses_an_expired_invite() {
    // The expiry is compared against a caller-supplied `now`, which is what
    // lets this reach the far side of a lifetime without sleeping.
    let (database, code) = setup(Some(Duration::minutes(10)), 0).await;
    let later = Utc::now() + Duration::minutes(11);

    assert_eq!(
        db::check_invite(&database, code, Utc::now())
            .await
            .expect("check"),
        Redemption::Granted
    );
    assert_eq!(
        db::check_invite(&database, code, later)
            .await
            .expect("check"),
        Redemption::Expired
    );
    assert_eq!(
        db::redeem_invite(&database, code, KEY, later)
            .await
            .expect("redeem"),
        Redemption::Expired
    );
    assert!(!db::is_invite_member(&database, KEY).await.expect("member"));
    assert_eq!(db::list_invites(&database).await.expect("list")[0].uses, 0);
}

#[tokio::test]
async fn stops_at_the_use_limit() {
    let (database, code) = setup(None, 1).await;

    assert_eq!(
        db::redeem_invite(&database, code, KEY, Utc::now())
            .await
            .expect("redeem"),
        Redemption::Granted
    );
    // The second key finds the invite spent — the counter is the gate, not
    // the number of distinct people who happen to hold the link.
    assert_eq!(
        db::redeem_invite(&database, code, OTHER_KEY, Utc::now())
            .await
            .expect("redeem"),
        Redemption::Exhausted
    );
    assert!(
        !db::is_invite_member(&database, OTHER_KEY)
            .await
            .expect("member")
    );
    assert_eq!(db::list_invites(&database).await.expect("list")[0].uses, 1);
}

#[tokio::test]
async fn revoking_stops_new_joins_but_keeps_existing_members() {
    let (database, code) = setup(None, 0).await;
    db::redeem_invite(&database, code, KEY, Utc::now())
        .await
        .expect("redeem");

    assert!(db::revoke_invite(&database, code).await.expect("revoke"));
    assert!(db::list_invites(&database).await.expect("list").is_empty());
    // Revoking twice is not an error the caller has to distinguish, but it
    // must report that there was nothing left to revoke.
    assert!(!db::revoke_invite(&database, code).await.expect("revoke"));

    // Nobody new gets in with the withdrawn code...
    assert_eq!(
        db::redeem_invite(&database, code, OTHER_KEY, Utc::now())
            .await
            .expect("redeem"),
        Redemption::Unknown
    );
    // ...while the key that already joined stays a member. This is what makes
    // a member's reconnect independent of the invite still existing.
    assert!(db::is_invite_member(&database, KEY).await.expect("member"));
}

#[tokio::test]
async fn a_members_reconnect_never_spends_another_use() {
    let (database, code) = setup(None, 1).await;
    db::redeem_invite(&database, code, KEY, Utc::now())
        .await
        .expect("redeem");

    // Presence checks membership before it looks at the code at all, so the
    // counter stays where it was however often the member comes back.
    for _ in 0..3 {
        assert!(db::is_invite_member(&database, KEY).await.expect("member"));
    }
    assert_eq!(db::list_invites(&database).await.expect("list")[0].uses, 1);
}

#[tokio::test]
async fn seeds_the_invite_permission_on_moderating_roles() {
    let database = db::init(":memory:").await.expect("in-memory db");
    let defs = db::list_role_defs(&database).await.expect("list roles");

    assert_eq!(DEFAULT_MOD & CREATE_INVITES, CREATE_INVITES);

    let everyone = defs.iter().find(|d| d.is_default).expect("@everyone");
    // Minting invites on a password-protected server admits new members
    // without the password, so the baseline role must never carry it.
    assert_eq!(everyone.permissions & CREATE_INVITES, 0);

    for def in defs.iter().filter(|d| d.permissions & KICK_MEMBERS != 0) {
        assert_eq!(
            def.permissions & CREATE_INVITES,
            CREATE_INVITES,
            "{} moderates members but cannot invite",
            def.name
        );
    }
}

#[tokio::test]
async fn the_invite_permission_migration_is_idempotent_and_leaves_others_alone() {
    let database = db::init(":memory:").await.expect("in-memory db");

    // A role that does not moderate must not gain the flag, on this run or a
    // later one — the migration is marker-guarded, so re-running the schema
    // must not hand it out a second time to roles an owner has since edited.
    let id = db::create_role_def(&database, "Lurker", None, VIEW_CHANNELS, 1)
        .await
        .expect("create role");

    db::run_schema(&database).await.expect("re-run schema");
    db::run_schema(&database)
        .await
        .expect("re-run schema again");

    let defs = db::list_role_defs(&database).await.expect("list roles");
    let lurker = defs.iter().find(|d| d.id == id).expect("custom role");
    assert_eq!(lurker.permissions, VIEW_CHANNELS);

    let everyone = defs.iter().find(|d| d.is_default).expect("@everyone");
    assert_eq!(everyone.permissions & CREATE_INVITES, 0);
}
