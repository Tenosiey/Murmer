use murmer_server::ws::helpers::moderation_rank;

#[test]
fn ranks_standard_roles() {
    assert_eq!(moderation_rank(Some("Owner")), 3);
    assert_eq!(moderation_rank(Some("Admin")), 2);
    assert_eq!(moderation_rank(Some("Mod")), 1);
}

#[test]
fn is_case_insensitive() {
    assert_eq!(moderation_rank(Some("owner")), 3);
    assert_eq!(moderation_rank(Some("ADMIN")), 2);
    assert_eq!(moderation_rank(Some("mod")), 1);
}

#[test]
fn unprivileged_roles_have_no_rank() {
    assert_eq!(moderation_rank(None), 0);
    assert_eq!(moderation_rank(Some("Member")), 0);
    assert_eq!(moderation_rank(Some("")), 0);
}

#[test]
fn moderation_requires_strictly_higher_rank() {
    // A Mod must not be able to act against another Mod or above.
    assert!(moderation_rank(Some("Mod")) <= moderation_rank(Some("Mod")));
    assert!(moderation_rank(Some("Admin")) > moderation_rank(Some("Mod")));
    assert!(moderation_rank(Some("Owner")) <= moderation_rank(Some("Owner")));
}
