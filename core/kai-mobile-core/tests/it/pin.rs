use kai_mobile_core::LmgwPin;

#[test]
fn the_built_in_pin_is_the_file_in_the_repository() {
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../LMGW_PIN")).unwrap();
    assert_eq!(LmgwPin::built_in(), LmgwPin::parse(&text).unwrap());
}

#[test]
fn a_malformed_pin_is_refused() {
    let ok = "lmgw_commit 0123456789abcdef0123456789abcdef01234567\nlmgw_tag v0.4.4\nlmgw_tree 89abcdef0123456789abcdef0123456789abcdef\n";
    assert!(LmgwPin::parse(ok).unwrap().is_published());
    assert!(LmgwPin::parse(&ok.replace("v0.4.4", "latest")).is_err());
    assert!(
        LmgwPin::parse(&ok.replace("0123456789abcdef0123456789abcdef01234567", "abc")).is_err()
    );
    assert!(LmgwPin::parse("lmgw_tag unpublished\n").is_err());
}
