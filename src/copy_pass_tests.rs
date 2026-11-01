use crate::ui::plural;

#[test]
fn plural_agrees_with_the_count() {
    assert_eq!(plural(0, "process", "processes"), "processes");
    assert_eq!(plural(1, "process", "processes"), "process");
    assert_eq!(plural(2, "process", "processes"), "processes");
    assert_eq!(plural(1, "Row", "Rows"), "Row");
    assert_eq!(plural(3, "Row", "Rows"), "Rows");
}
