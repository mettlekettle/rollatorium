mod common;
use common::r;

// ============================================================================
// Invalid Rolls
// ============================================================================

#[test]
#[should_panic(expected = "Exceeded maximum number of rolls")]
fn test_too_many_rolls() {
    let _ = r("1001d6");
}

#[test]
#[should_panic(expected = "die size must be positive")]
fn test_zero_sided_die() {
    let _ = r("6d0");
}

#[test]
#[should_panic(expected = "selector target must be positive")]
fn test_invalid_minimum() {
    let _ = r("10d6mil1");
}
