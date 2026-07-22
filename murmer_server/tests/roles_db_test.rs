//! Integration tests for the role-definition and assignment persistence layer,
//! including the one-time seed run by `db::init`.

use murmer_server::db;
use murmer_server::permissions::{
    self, ADMINISTRATOR, DEFAULT_EVERYONE, MANAGE_SERVER, SEND_MESSAGES, VIEW_CHANNELS,
};
use murmer_server::roles::EVERYONE_ROLE_NAME;

#[tokio::test]
async fn seeds_builtin_roles_on_init() {
    let database = db::init(":memory:").await.expect("in-memory db");
    let defs = db::list_role_defs(&database).await.expect("list roles");

    let everyone = defs
        .iter()
        .find(|d| d.is_default)
        .expect("default @everyone role seeded");
    assert_eq!(everyone.name, EVERYONE_ROLE_NAME);
    assert_eq!(everyone.permissions, DEFAULT_EVERYONE);
    assert!(permissions::mask_allows(
        everyone.permissions,
        VIEW_CHANNELS
    ));
    assert!(permissions::mask_allows(
        everyone.permissions,
        SEND_MESSAGES
    ));

    let owner = defs.iter().find(|d| d.is_owner).expect("owner role seeded");
    assert_eq!(owner.permissions & ADMINISTRATOR, ADMINISTRATOR);
    // Owner outranks every other seeded role.
    assert!(
        defs.iter()
            .all(|d| d.is_owner || d.position < owner.position)
    );

    // The named built-ins exist and are ordered below the owner.
    for name in ["Mod", "Admin"] {
        let def = db::get_role_def_by_name(&database, name)
            .await
            .expect("query")
            .unwrap_or_else(|| panic!("{name} seeded"));
        assert!(!def.is_default && !def.is_owner);
        assert!(def.position < owner.position);
    }
}

#[tokio::test]
async fn create_and_assign_custom_role() {
    let database = db::init(":memory:").await.expect("in-memory db");
    let key = "test-public-key";

    // A view-only "Dude" role (the motivating example).
    let id = db::create_role_def(&database, "Dude", Some("#abcdef"), VIEW_CHANNELS, 1)
        .await
        .expect("create role");

    db::set_user_roles(&database, key, &[id])
        .await
        .expect("assign role");
    assert_eq!(
        db::get_user_role_ids(&database, key).await.expect("read"),
        vec![id]
    );

    // Updating the permission mask persists.
    db::update_role_def(
        &database,
        id,
        "Dude",
        Some("#abcdef"),
        VIEW_CHANNELS | MANAGE_SERVER,
    )
    .await
    .expect("update role");
    let def = db::get_role_def(&database, id)
        .await
        .expect("query")
        .expect("still present");
    assert!(permissions::mask_allows(def.permissions, MANAGE_SERVER));

    // Deleting the role cascades to the assignment.
    assert!(db::delete_role_def(&database, id).await.expect("delete"));
    assert!(
        db::get_user_role_ids(&database, key)
            .await
            .expect("read")
            .is_empty()
    );
}

#[tokio::test]
async fn assign_named_role_bootstraps_owner() {
    let database = db::init(":memory:").await.expect("in-memory db");
    let key = "owner-key";

    let def = db::assign_named_role(&database, key, "Owner", None)
        .await
        .expect("assign owner");
    assert!(def.is_owner);
    assert!(
        db::get_user_role_ids(&database, key)
            .await
            .expect("read")
            .contains(&def.id)
    );

    // A previously unknown name creates a baseline custom role.
    let custom = db::assign_named_role(&database, key, "Streamer", None)
        .await
        .expect("assign custom");
    assert!(!custom.is_owner && !custom.is_default);
    assert_eq!(custom.permissions, DEFAULT_EVERYONE);
}
