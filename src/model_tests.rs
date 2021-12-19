#[allow(unused_imports)]
use crate::api::*;

#[test]
fn parse_time_string_tests() {
    fn parse_time_string(time_str: &str, now: i64) -> Option<i64> {
        serde_json::from_str::<Time>(time_str)
            .map(|t| t.absolute(now))
            .ok()
    }
    assert_eq!(parse_time_string(r#"-0"#, 0), Some(0));
    assert_eq!(parse_time_string(r#"-12"#, 0), Some(-12));
    assert_eq!(parse_time_string(r#""-0""#, 0), Some(0));
    assert_eq!(parse_time_string(r#""-12""#, 0), Some(-12));
    assert_eq!(
        parse_time_string(r#""9223372036854775807""#, 0),
        Some(9223372036854775807)
    );
    assert_eq!(parse_time_string(r#""9223372036854775808""#, 0), None);
    assert_eq!(parse_time_string(r#""now""#, 123), Some(123));
    assert_eq!(parse_time_string(r#""now+3""#, 123), Some(126));
    assert_eq!(parse_time_string(r#""now+0""#, 123), Some(123));
    assert_eq!(parse_time_string(r#""now-3""#, 123), Some(120));
}
