#[allow(unused_imports)]
use model::*;

#[test]
fn parse_time_string_tests() {
    assert_eq!(parse_time_string("-0", 0), Ok(0));
    assert_eq!(parse_time_string("-12", 0), Ok(-12));
    assert_eq!(
        parse_time_string("9223372036854775808", 0),
        Err("bad time specification".into())
    );
    assert_eq!(parse_time_string("now", 123), Ok(123));
    assert_eq!(parse_time_string("now+3", 123), Ok(126));
    assert_eq!(parse_time_string("now+0", 123), Ok(123));
    assert_eq!(parse_time_string("now-3", 123), Ok(120));
}
