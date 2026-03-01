mod epd_common;

/// Bratko-Kopec test suite — 24 tactical + strategic positions.
///
/// Run: cargo test puzzle_bk -- --ignored --nocapture --test-threads=1
#[test]
#[ignore]
fn puzzle_bk() {
    let data = include_str!("data/bk.epd");
    let (passed, total) = epd_common::run_epd_suite(data, "Bratko-Kopec");
    assert!(
        passed as f64 / total.max(1) as f64 > 0.3,
        "BK pass rate {passed}/{total} is below 30%"
    );
}
