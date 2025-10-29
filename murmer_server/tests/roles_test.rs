use murmer_server::roles::default_color;

#[test]
fn provides_default_colors_for_standard_roles() {
    assert_eq!(default_color("Admin"), Some("#eab308".to_string()));
    assert_eq!(default_color("Mod"), Some("#10b981".to_string()));
    assert_eq!(default_color("Owner"), Some("#3b82f6".to_string()));
}

#[test]
fn returns_none_for_custom_roles() {
    assert_eq!(default_color("CustomRole"), None);
    assert_eq!(default_color("Member"), None);
    assert_eq!(default_color("Guest"), None);
}

#[test]
fn is_case_insensitive_for_standard_roles() {
    assert_eq!(default_color("admin"), Some("#eab308".to_string()));
    assert_eq!(default_color("ADMIN"), Some("#eab308".to_string()));
    assert_eq!(default_color("Admin"), Some("#eab308".to_string()));
}

