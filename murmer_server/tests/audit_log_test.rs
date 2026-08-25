//! Tests for the audit log: the record of who moderated whom and who changed
//! the server's shape.
//!
//! Everything worth pinning here is invisible from the app. The log is only
//! consulted after something went wrong, which is precisely when nobody is
//! in a position to notice that it has been quietly wrong for weeks:
//!
//! - **retention** trims the *oldest* rows. A trim that took the newest ones,
//!   or that failed to run at all, both look like a working log right up
//!   until the disk fills or the entry somebody needs is gone;
//! - **a reset does not clear it.** The record of a wipe is worth most
//!   immediately after one, and an action that erased its own trace would
//!   make the whole feature worthless;
//! - **the permission is its own flag**, seeded into the admin tier and
//!   granted to existing admin-tier roles exactly once — a migration that
//!   re-granted a deliberately revoked flag on every restart would silently
//!   undo an owner's decision;
//! - **the `/role` sentinel is unforgeable**, or an entry made by the bearer
//!   token could be mistaken for one made by a member.

use murmer_server::db;
use murmer_server::db::DbCall;
use murmer_server::db::actions;
use murmer_server::permissions::{
    self, DEFAULT_ADMIN, DEFAULT_EVERYONE, DEFAULT_MOD, MANAGE_SERVER, VIEW_AUDIT_LOG,
};
use murmer_server::security::validate_user_name;

async fn setup() -> db::Db {
    db::init(":memory:").await.expect("in-memory db")
}

async fn record(db: &db::Db, action: &str, actor: &str, target: &str, detail: &str) {
    db::record_audit_entry(db, action, actor, target, detail)
        .await
        .expect("record audit entry");
}

async fn role_permissions(db: &db::Db, name: &str) -> u64 {
    db::get_role_def_by_name(db, name)
        .await
        .expect("lookup")
        .expect("role exists")
        .permissions
}

async fn role_id(db: &db::Db, name: &str) -> i64 {
    db::get_role_def_by_name(db, name)
        .await
        .expect("lookup")
        .expect("role exists")
        .id
}

#[tokio::test]
async fn records_an_entry_and_answers_with_it() {
    let db = setup().await;

    record(&db, actions::BAN, "mod", "spammer", "").await;
    record(
        &db,
        actions::MUTE,
        "mod",
        "raider",
        "until 2026-08-01T10:00:00Z",
    )
    .await;

    let entries = db::list_audit_entries(&db).await.expect("list");
    assert_eq!(entries.len(), 2);
    // Newest first: the dashboard shows the log top-down and the recent end is
    // the one anybody is looking for.
    assert_eq!(entries[0].action, actions::MUTE);
    assert_eq!(entries[0].actor, "mod");
    assert_eq!(entries[0].target, "raider");
    assert_eq!(entries[0].detail, "until 2026-08-01T10:00:00Z");
    assert_eq!(entries[1].action, actions::BAN);
    assert!(entries[0].id > entries[1].id);
}

#[tokio::test]
async fn trims_the_oldest_entries_once_the_log_is_full() {
    let db = setup().await;

    // A handful past the cap is enough: the trim runs on every insert, so if
    // it takes the wrong end it does so on the first row over.
    let overflow = 5;
    for index in 0..db::MAX_AUDIT_ENTRIES + overflow {
        record(&db, actions::KICK, "mod", &format!("user-{index}"), "").await;
    }

    let total: i64 = db
        .call_db(|conn| conn.query_row("SELECT COUNT(*) FROM audit_log", [], |row| row.get(0)))
        .await
        .expect("count");
    assert_eq!(total, db::MAX_AUDIT_ENTRIES);

    let entries = db::list_audit_entries(&db).await.expect("list");
    // The newest survived...
    assert_eq!(
        entries[0].target,
        format!("user-{}", db::MAX_AUDIT_ENTRIES + overflow - 1)
    );
    // ...and the ones that fell off the end are the oldest, not the newest.
    let oldest: String = db
        .call_db(|conn| {
            conn.query_row(
                "SELECT target FROM audit_log ORDER BY id ASC LIMIT 1",
                [],
                |row| row.get(0),
            )
        })
        .await
        .expect("oldest");
    assert_eq!(oldest, format!("user-{overflow}"));
}

#[tokio::test]
async fn answers_at_most_one_page() {
    let db = setup().await;

    for index in 0..db::AUDIT_PAGE_SIZE + 10 {
        record(&db, actions::KICK, "mod", &format!("user-{index}"), "").await;
    }

    let entries = db::list_audit_entries(&db).await.expect("list");
    assert_eq!(entries.len() as i64, db::AUDIT_PAGE_SIZE);
    // The page is taken off the recent end, not the old one.
    assert_eq!(
        entries[0].target,
        format!("user-{}", db::AUDIT_PAGE_SIZE + 9)
    );
}

#[tokio::test]
async fn a_reset_keeps_the_log() {
    let db = setup().await;

    record(&db, actions::BAN, "mod", "spammer", "").await;
    db::reset_server(&db).await.expect("reset");

    let entries = db::list_audit_entries(&db).await.expect("list");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].action, actions::BAN);
}

#[tokio::test]
async fn a_purge_keeps_the_log() {
    let db = setup().await;

    record(&db, actions::BAN, "mod", "spammer", "").await;
    db::purge_all_messages(&db).await.expect("purge");

    assert_eq!(db::list_audit_entries(&db).await.expect("list").len(), 1);
}

#[test]
fn the_view_flag_sits_with_the_admin_tier() {
    // The log is the record *of* the moderators, so a Mod does not get it by
    // default and @everyone must never have it — the rows name who moderated
    // whom.
    assert!(permissions::mask_allows(DEFAULT_ADMIN, VIEW_AUDIT_LOG));
    assert!(!permissions::mask_allows(DEFAULT_MOD, VIEW_AUDIT_LOG));
    assert!(!permissions::mask_allows(DEFAULT_EVERYONE, VIEW_AUDIT_LOG));
    // A client sending a role mask with this bit must not be refused for it.
    assert!(permissions::is_valid_mask(VIEW_AUDIT_LOG));
}

#[tokio::test]
async fn pre_audit_databases_grant_the_flag_to_admin_tier_roles_once() {
    let db = setup().await;

    // Simulate a database created before the audit log: clear the bit
    // everywhere and drop the marker so the migration runs again.
    let cleared = !VIEW_AUDIT_LOG as i64;
    db.call_db(move |conn| {
        conn.execute(
            "UPDATE role_definitions SET permissions = permissions & ?1",
            [cleared],
        )?;
        conn.execute(
            "DELETE FROM server_settings WHERE key = 'audit_log_perms'",
            [],
        )?;
        Ok(())
    })
    .await
    .expect("age the database");

    assert!(role_permissions(&db, "Admin").await & VIEW_AUDIT_LOG == 0);

    db::run_schema(&db).await.expect("re-run schema");

    // Roles that already administer the server can read the log...
    let admin = role_permissions(&db, "Admin").await;
    assert!(admin & MANAGE_SERVER != 0);
    assert!(admin & VIEW_AUDIT_LOG != 0);
    // ...and nobody else gains it. A Mod moderates but does not oversee.
    assert!(role_permissions(&db, "Mod").await & VIEW_AUDIT_LOG == 0);
    assert!(role_permissions(&db, "@everyone").await & VIEW_AUDIT_LOG == 0);

    // The marker makes it one-shot: a deliberate revocation must stick.
    let admin_id = role_id(&db, "Admin").await;
    db::update_role_def(&db, admin_id, "Admin", None, None, admin & !VIEW_AUDIT_LOG)
        .await
        .expect("revoke");
    db::run_schema(&db).await.expect("re-run schema again");
    assert!(role_permissions(&db, "Admin").await & VIEW_AUDIT_LOG == 0);
}

#[test]
fn the_admin_token_actor_cannot_be_an_account_name() {
    // The `/role` endpoint has no account behind it, so its entries are
    // attributed to a sentinel. If a member could register that name, an entry
    // they made would be indistinguishable from one the bearer token made.
    assert!(!validate_user_name(db::ACTOR_ADMIN_TOKEN));
}
