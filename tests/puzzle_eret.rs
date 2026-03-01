mod epd_common;

/// Eigenmann Rapid Engine Test — 110 positions (tactics, strategy, endgame).
///
/// Run: cargo test puzzle_eret -- --ignored --nocapture --test-threads=1
#[test]
#[ignore]
fn puzzle_eret() {
    let data = include_str!("data/eret.epd");
    let (passed, total) = epd_common::run_epd_suite(data, "ERET");
    assert!(
        passed as f64 / total.max(1) as f64 > 0.2,
        "ERET pass rate {passed}/{total} is below 20%"
    );
}
