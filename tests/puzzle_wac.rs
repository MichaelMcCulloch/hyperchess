mod epd_common;

/// Win At Chess — 300 tactical positions (the standard engine benchmark).
///
/// Run: cargo test puzzle_wac -- --ignored --nocapture --test-threads=1
#[test]
#[ignore]
fn puzzle_wac() {
    let data = include_str!("data/wac.epd");
    let (passed, total) = epd_common::run_epd_suite(data, "WAC");
    assert!(
        passed as f64 / total.max(1) as f64 > 0.3,
        "WAC pass rate {passed}/{total} is below 30%"
    );
}
