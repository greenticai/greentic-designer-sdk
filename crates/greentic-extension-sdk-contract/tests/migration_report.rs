use greentic_extension_sdk_contract::migration::MigrationReport;

#[test]
fn empty_report_has_no_warnings() {
    let r = MigrationReport::default();
    assert!(r.warnings.is_empty());
    assert!(r.dropped_keys.is_empty());
}

#[test]
fn report_pushes_warnings() {
    let mut r = MigrationReport::default();
    r.warn("oh no");
    r.dropped("targets");
    assert_eq!(r.warnings.len(), 1);
    assert_eq!(r.dropped_keys, vec!["targets".to_string()]);
}
