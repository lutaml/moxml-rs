//! Lexical validation for the XSD built-in simple types
//! (Datatypes Part 2) — the Rust mirror of lutaml/moxml's
//! `Moxml::XSD::BuiltinLexical`. One committed answer: the check is
//! lexical only; pattern/enumeration facets belong to the schema.
//!
//! Every predicate is hand-rolled (no regex dependency) and mirrors
//! the Ruby implementation's structure so the two read the same.

/// Error for a name outside the table — the mirror of the Ruby
/// side's ArgumentError ("the facade never guesses").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownBuiltin(pub String);

impl std::fmt::Display for UnknownBuiltin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown built-in type {:?}", self.0)
    }
}

impl std::error::Error for UnknownBuiltin {}

/// Lexical validation against the table ("xs:NAME" reference
/// spelling). Unknown names return [`UnknownBuiltin`] — the facade
/// never guesses.
pub fn valid(builtin: &str, lexical: &str) -> Result<bool, UnknownBuiltin> {
    match checker(builtin) {
        Some(check) => Ok(check(lexical)),
        None => Err(UnknownBuiltin(builtin.to_string())),
    }
}

/// The supported "xs:NAME" spellings (sorted).
pub fn names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = TABLE_KEYS.to_vec();
    names.sort_unstable();
    names
}

const TABLE_KEYS: [&str; 50] = [
    "xs:anyAtomicType",
    "xs:anySimpleType",
    "xs:anyType",
    "xs:anyURI",
    "xs:base64Binary",
    "xs:boolean",
    "xs:byte",
    "xs:date",
    "xs:dateTime",
    "xs:dateTimeStamp",
    "xs:dayTimeDuration",
    "xs:decimal",
    "xs:double",
    "xs:duration",
    "xs:ENTITIES",
    "xs:ENTITY",
    "xs:float",
    "xs:gDay",
    "xs:gMonth",
    "xs:gMonthDay",
    "xs:gYear",
    "xs:gYearMonth",
    "xs:hexBinary",
    "xs:ID",
    "xs:IDREF",
    "xs:IDREFS",
    "xs:int",
    "xs:integer",
    "xs:language",
    "xs:long",
    "xs:Name",
    "xs:NCName",
    "xs:negativeInteger",
    "xs:NMTOKEN",
    "xs:NMTOKENS",
    "xs:nonNegativeInteger",
    "xs:nonPositiveInteger",
    "xs:NOTATION",
    "xs:positiveInteger",
    "xs:precisionDecimal",
    "xs:QName",
    "xs:short",
    "xs:string",
    "xs:time",
    "xs:token",
    "xs:unsignedByte",
    "xs:unsignedInt",
    "xs:unsignedLong",
    "xs:unsignedShort",
    "xs:yearMonthDuration",
];

type Check = fn(&str) -> bool;

/// whiteSpace=collapse, shared by token and its derivatives.
fn collapse(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_space = false;
    for c in s.chars() {
        if matches!(c, ' ' | '\t' | '\r' | '\n') {
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(c);
        }
    }
    out
}

fn digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// `digits[.digits]` — integer part required, fraction optional, at
/// most one dot (the seconds' fractional form).
fn decimal_digits(s: &str) -> bool {
    match s.split_once('.') {
        Some((int_part, frac_part)) => digits(int_part) && digits(frac_part),
        None => digits(s),
    }
}

/// XML 1.0 5th ed. NCName start (no ":").
fn ncname_start(c: char) -> bool {
    matches!(c,
        'A'..='Z' | '_' | 'a'..='z'
        | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}'
        | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}'
        | '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}'
        | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}'
        | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}')
}

/// XML 1.0 5th ed. NCName char (no ":").
fn ncname_char(c: char) -> bool {
    ncname_start(c)
        || matches!(c,
            '0'..='9' | '.' | '-' | '\u{B7}' | '\u{300}'..='\u{36F}'
            | '\u{203F}' | '\u{2040}')
}

fn ncname(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if ncname_start(c) => chars.all(ncname_char),
        _ => false,
    }
}

/// xs:Name admits ":" in every position.
fn name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c == ':' || ncname_start(c) => chars.all(|c| c == ':' || ncname_char(c)),
        _ => false,
    }
}

/// xs:NMTOKEN: NameChar+ (":" admitted).
fn nmtoken(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c == ':' || ncname_char(c))
}

/// A QName is an optional NCName prefix, ":", and an NCName local
/// part. The "xmlns" prefix is reserved (Namespaces in XML) and never
/// a legal QName prefix.
fn qname(s: &str) -> bool {
    match s.split_once(':') {
        None => ncname(s),
        Some((prefix, local)) => prefix != "xmlns" && ncname(prefix) && ncname(local),
    }
}

/// List lexical spaces: whitespace-separated items, at least one;
/// every item must satisfy the item predicate.
fn list(s: &str, item: fn(&str) -> bool) -> bool {
    let items: Vec<&str> = s
        .split([' ', '\t', '\r', '\n'])
        .filter(|i| !i.is_empty())
        .collect();
    !items.is_empty() && items.iter().all(|i| item(i))
}

/// The integer lexical (`[+-]?[0-9]+`) with an optional inclusive
/// bound check. Unbounded callers pass `None`. A parse overflow past
/// i128 is outside every bound.
fn integer(s: &str, bound: Option<(i128, i128)>) -> bool {
    let (negative, unsigned) = match s.as_bytes().first() {
        Some(b'+') => (false, &s[1..]),
        Some(b'-') => (true, &s[1..]),
        _ => (false, s),
    };
    if !digits(unsigned) {
        return false;
    }
    match bound {
        None => true,
        Some((low, high)) => match unsigned.parse::<i128>() {
            Ok(v) => {
                let v = if negative { -v } else { v };
                (low..=high).contains(&v)
            }
            Err(_) => false,
        },
    }
}

/// The float lexical family: `[+-]?(digits[.digits?]|.digits)`
/// plus an optional `[Ee][+-]?digits` exponent; `allow_specials`
/// admits the INF/NaN spellings (the decimal face has none).
fn float_like(s: &str, allow_specials: bool) -> bool {
    if allow_specials && matches!(s, "INF" | "-INF" | "+INF" | "NaN") {
        return true;
    }
    let (mantissa, exponent) = match s.split_once(['E', 'e']) {
        Some((m, e)) => (m, Some(e)),
        None => (s, None),
    };
    if let Some(e) = exponent {
        let e = e
            .strip_prefix('+')
            .or_else(|| e.strip_prefix('-'))
            .unwrap_or(e);
        if !digits(e) {
            return false;
        }
    }
    let m = mantissa
        .strip_prefix('+')
        .or_else(|| mantissa.strip_prefix('-'))
        .unwrap_or(mantissa);
    match m.split_once('.') {
        Some((int_part, frac_part)) => {
            (digits(int_part) || int_part.is_empty())
                && (digits(frac_part) || frac_part.is_empty())
                && !(int_part.is_empty() && frac_part.is_empty())
        }
        None => digits(m),
    }
}

/// Zone-tail handling: `Some((body, ok))` when the string ends with a
/// zone (valid or an invalid attempt), `None` when no zone is present.
/// The six-from-the-end offset can never be a negative date's own year
/// sign: that sign sits at position 0, and the colon at +3 pins the
/// zone shape.
fn take_zone(s: &str) -> Option<(&str, bool)> {
    if let Some(body) = s.strip_suffix('Z') {
        return Some((body, true));
    }
    if s.len() >= 7 {
        let i = s.len() - 6;
        let b = s.as_bytes();
        if (b[i] == b'+' || b[i] == b'-') && b[i + 3] == b':' {
            return Some((&s[..i], zone(&s[i..])));
        }
    }
    None
}

/// The timezone lexical: `Z`, `[+-]hh:mm` (00:00–13:59), or the
/// unsigned `14:00`.
fn zone(s: &str) -> bool {
    if s == "Z" || s == "14:00" {
        return true;
    }
    let Some(hhmm) = s.strip_prefix(['+', '-']) else {
        return false;
    };
    let Some((hh, mm)) = hhmm.split_once(':') else {
        return false;
    };
    if hh.len() != 2 || mm.len() != 2 {
        return false;
    }
    let (Some(h), Some(m)) = (hh.parse::<u32>().ok(), mm.parse::<u32>().ok()) else {
        return false;
    };
    (h <= 13 && m <= 59) || (h == 14 && m == 0)
}

fn two_digits(s: &str, low: u32, high: u32) -> Option<u32> {
    if s.len() == 2 && digits(s) {
        let v: u32 = s.parse().ok()?;
        if (low..=high).contains(&v) {
            return Some(v);
        }
    }
    None
}

fn leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year(y.abs()) => 29,
        2 => 28,
        _ => 0,
    }
}

const MONTH_DAYS: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// Dates carry calendar validity in their lexical space: the day must
/// exist for the month and year (Gregorian, leap rules on the
/// absolute year).
fn calendar_date(y: i64, m: u32, d: u32) -> bool {
    (1..=12).contains(&m) && (1..=days_in_month(y, m)).contains(&d)
}

fn year_digits(s: &str) -> bool {
    s.len() >= 4 && s.bytes().all(|b| b.is_ascii_digit())
}

/// The date part: optional `-`, a 4+ digit year, `-mm-dd`, calendar-
/// valid. Answers `true/false` (the caller anchors the zone).
/// dateTime = date `T` time.
fn date_time(s: &str) -> bool {
    match s.split_once('T') {
        Some((date, time)) => date_part(date) && time_part(time),
        None => false,
    }
}

fn date_part(s: &str) -> bool {
    let (negative, unsigned) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s),
    };
    let Some((y, rest)) = unsigned.split_once('-') else {
        return false;
    };
    if !year_digits(y) {
        return false;
    }
    let year: i64 = if negative {
        format!("-{y}").parse().unwrap_or(i64::MAX)
    } else {
        y.parse().unwrap_or(i64::MAX)
    };
    let Some((mm, dd)) = rest.split_once('-') else {
        return false;
    };
    let Some(month) = two_digits(mm, 1, 12) else {
        return false;
    };
    let Some(day) = two_digits(dd, 1, 31) else {
        return false;
    };
    calendar_date(year, month, day)
}

/// `hh:mm:ss(.fff)?` — hh 00-23, mm/ss 00-59, fraction digits only.
fn time_part(s: &str) -> bool {
    let (hms, frac) = match s.split_once('.') {
        Some((hms, f)) => (hms, Some(f)),
        None => (s, None),
    };
    if let Some(f) = frac {
        if !digits(f) {
            return false;
        }
    }
    let parts: Vec<&str> = hms.split(':').collect();
    parts.len() == 3
        && two_digits(parts[0], 0, 23).is_some()
        && two_digits(parts[1], 0, 59).is_some()
        && two_digits(parts[2], 0, 59).is_some()
}

/// The duration grammar with the component rules: `-?P` then ordered
/// Y/M/D runs (digits only, one per kind), an optional `T` section
/// with ordered H/M/S runs (fraction on S only), T requires a
/// following component, at least one component overall.
fn duration(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    let Some(rest) = body.strip_prefix('P') else {
        return false;
    };
    let (day, time) = match rest.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (rest, None),
    };
    let mut components = 0usize;
    let mut d = day;
    for suffix in ['Y', 'M', 'D'] {
        if let Some(pos) = d.find(suffix) {
            if !digits(&d[..pos]) {
                return false;
            }
            components += 1;
            d = &d[pos + 1..];
        }
    }
    if !d.is_empty() {
        return false;
    }
    let mut time_components = 0usize;
    if let Some(t) = time {
        let mut t = t;
        for (suffix, fractional) in [('H', false), ('M', false), ('S', true)] {
            if let Some(pos) = t.find(suffix) {
                let value = &t[..pos];
                if fractional {
                    if !decimal_digits(value) {
                        return false;
                    }
                } else if !digits(value) {
                    return false;
                }
                time_components += 1;
                t = &t[pos + 1..];
            }
        }
        if !t.is_empty() || time_components == 0 {
            return false;
        }
    }
    components + time_components >= 1
}

/// The decimal shape: sign, digits (or a leading-dot form), optional
/// fraction — NO exponent (the float face's difference).
fn decimal_shape(s: &str) -> bool {
    let m = s.strip_prefix(['+', '-']).unwrap_or(s);
    match m.split_once('.') {
        Some((int_part, frac_part)) => {
            if int_part.is_empty() {
                digits(frac_part)
            } else {
                digits(int_part) && (frac_part.is_empty() || digits(frac_part))
            }
        }
        None => digits(m),
    }
}

/// The checker table: the "xs:NAME" spellings, mirroring the Ruby
/// CHECKERS order and coverage.
fn checker(builtin: &str) -> Option<Check> {
    let f: Check = match builtin {
        "xs:anySimpleType"
        | "xs:anyAtomicType"
        | "xs:anyType"
        | "xs:string"
        | "xs:normalizedString"
        | "xs:token" => |_| true,
        "xs:language" => |s| {
            let s = collapse(s);
            if s.is_empty() {
                return false;
            }
            s.split('-').all(|sub| {
                (1..=8).contains(&(sub.len() as u32))
                    && sub.chars().all(|c| c.is_ascii_alphanumeric())
            }) && s
                .split('-')
                .next()
                .unwrap_or("")
                .chars()
                .all(|c| c.is_ascii_alphabetic())
        },
        "xs:Name" => |s| name(&collapse(s)),
        "xs:NCName" | "xs:ID" | "xs:IDREF" | "xs:ENTITY" => |s| ncname(&collapse(s)),
        "xs:NMTOKEN" => |s| nmtoken(&collapse(s)),
        "xs:IDREFS" => |s| list(s, ncname),
        "xs:ENTITIES" => |s| list(s, ncname),
        "xs:NMTOKENS" => |s| list(s, nmtoken),
        "xs:boolean" => |s| matches!(s, "true" | "false" | "0" | "1"),
        "xs:decimal" => |s| decimal_shape(s),
        "xs:precisionDecimal" => |s| float_like(s, true),
        "xs:float" | "xs:double" => |s| float_like(s, true),
        "xs:integer" => |s| integer(s, None),
        "xs:nonPositiveInteger" => |s| integer(s, Some((i128::MIN, 0))),
        "xs:negativeInteger" => |s| integer(s, Some((i128::MIN, -1))),
        "xs:long" => |s| integer(s, Some((i128::from(i64::MIN), i128::from(i64::MAX)))),
        "xs:int" => |s| integer(s, Some((i128::from(i32::MIN), i128::from(i32::MAX)))),
        "xs:short" => |s| integer(s, Some((-32_768, 32_767))),
        "xs:byte" => |s| integer(s, Some((-128, 127))),
        "xs:nonNegativeInteger" => |s| integer(s, Some((0, i128::MAX))),
        "xs:positiveInteger" => |s| integer(s, Some((1, i128::MAX))),
        "xs:unsignedLong" => |s| integer(s, Some((0, i128::from(u64::MAX)))),
        "xs:unsignedInt" => |s| integer(s, Some((0, i128::from(u32::MAX)))),
        "xs:unsignedShort" => |s| integer(s, Some((0, 65_535))),
        "xs:unsignedByte" => |s| integer(s, Some((0, 255))),
        "xs:duration" => duration,
        "xs:dayTimeDuration" => |s| {
            duration(s) && {
                let body = s
                    .strip_prefix('-')
                    .unwrap_or(s)
                    .strip_prefix('P')
                    .unwrap_or(s);
                let head = body.split('T').next().unwrap_or(body);
                !head.contains('Y') && !head.contains('M')
            }
        },
        "xs:yearMonthDuration" => |s| {
            duration(s) && !s.contains('D') && !s.contains('T') && {
                let body = s
                    .strip_prefix('-')
                    .unwrap_or(s)
                    .strip_prefix('P')
                    .unwrap_or(s);
                !body.is_empty() && body.contains('Y') || body.contains('M')
            }
        },
        "xs:dateTime" => |s| match take_zone(s) {
            None => date_time(s),
            Some((body, true)) => date_time(body),
            Some((_, false)) => false,
        },
        "xs:dateTimeStamp" => |s| match take_zone(s) {
            Some((body, true)) => date_time(body),
            _ => false,
        },
        "xs:time" => |s| match take_zone(s) {
            None => time_part(s),
            Some((body, true)) => time_part(body),
            Some((_, false)) => false,
        },
        "xs:date" => |s| match take_zone(s) {
            None => date_part(s),
            Some((body, true)) => date_part(body),
            Some((_, false)) => false,
        },
        "xs:gYear" => |s| match take_zone(s) {
            None => {
                let body = s;
                let y = body.strip_prefix('-').unwrap_or(body);
                year_digits(y) && (y.len() == 4 || !y.starts_with('0'))
            }
            Some((body, true)) => {
                let y = body.strip_prefix('-').unwrap_or(body);
                year_digits(y) && (y.len() == 4 || !y.starts_with('0'))
            }
            Some((_, false)) => false,
        },
        "xs:gYearMonth" => |s| match take_zone(s) {
            None => {
                let body = s;
                let unsigned = body.strip_prefix('-').unwrap_or(body);
                match unsigned.split_once('-') {
                    Some((y, m)) => year_digits(y) && two_digits(m, 1, 12).is_some(),
                    None => false,
                }
            }
            Some((body, true)) => {
                let unsigned = body.strip_prefix('-').unwrap_or(body);
                match unsigned.split_once('-') {
                    Some((y, m)) => year_digits(y) && two_digits(m, 1, 12).is_some(),
                    None => false,
                }
            }
            Some((_, false)) => false,
        },
        "xs:gMonth" => |s| match take_zone(s) {
            None => {
                let body = s;
                body.strip_prefix("--")
                    .and_then(|m| two_digits(m, 1, 12))
                    .is_some()
            }
            Some((body, true)) => body
                .strip_prefix("--")
                .and_then(|m| two_digits(m, 1, 12))
                .is_some(),
            Some((_, false)) => false,
        },
        "xs:gDay" => |s| match take_zone(s) {
            None => {
                let body = s;
                body.strip_prefix("---")
                    .and_then(|d| two_digits(d, 1, 31))
                    .is_some()
            }
            Some((body, true)) => body
                .strip_prefix("---")
                .and_then(|d| two_digits(d, 1, 31))
                .is_some(),
            Some((_, false)) => false,
        },
        "xs:gMonthDay" => |s| match take_zone(s) {
            None => {
                let body = s;
                let Some(rest) = body.strip_prefix("--") else {
                    return false;
                };
                let Some((m, d)) = rest.split_once('-') else {
                    return false;
                };
                let Some(month) = two_digits(m, 1, 12) else {
                    return false;
                };
                let Some(day) = two_digits(d, 1, 31) else {
                    return false;
                };
                day <= if month == 2 {
                    29
                } else {
                    MONTH_DAYS[(month - 1) as usize]
                }
            }
            Some((body, true)) => {
                let Some(rest) = body.strip_prefix("--") else {
                    return false;
                };
                let Some((m, d)) = rest.split_once('-') else {
                    return false;
                };
                let Some(month) = two_digits(m, 1, 12) else {
                    return false;
                };
                let Some(day) = two_digits(d, 1, 31) else {
                    return false;
                };
                day <= if month == 2 {
                    29
                } else {
                    MONTH_DAYS[(month - 1) as usize]
                }
            }
            Some((_, false)) => false,
        },
        "xs:hexBinary" => {
            |s| s.is_empty() || (s.len() % 2 == 0 && s.chars().all(|c| c.is_ascii_hexdigit()))
        }
        "xs:base64Binary" => base64_binary,
        "xs:anyURI" => |_| true,
        "xs:QName" | "xs:NOTATION" => |s| qname(&collapse(s)),
        _ => return None,
    };
    Some(f)
}

/// Base64: whitespace stripped, then 4-char quanta with the one-or-two
/// `=` padding shapes allowed only in the final quantum; alphabet
/// A-Za-z0-9+/.
fn base64_binary(s: &str) -> bool {
    let stripped: String = s
        .chars()
        .filter(|c| !matches!(c, ' ' | '\t' | '\r' | '\n'))
        .collect();
    if stripped.is_empty() {
        return true;
    }
    if !stripped.len().is_multiple_of(4) {
        return false;
    }
    let quanta: Vec<&[u8]> = stripped.as_bytes().chunks(4).collect();
    for (i, quantum) in quanta.iter().enumerate() {
        let pads = quantum.iter().filter(|b| **b == b'=').count();
        let last = i == quanta.len() - 1;
        match pads {
            0 => {
                if !quantum.iter().all(|b| base64_char(*b)) {
                    return false;
                }
            }
            1 | 2 => {
                if !last || !quantum[..4 - pads].iter().all(|b| base64_char(*b)) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

fn base64_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'+' || b == b'/'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_works() {
        assert_eq!(collapse(" id "), "id");
        assert_eq!(collapse("a  b"), "a b");
    }

    #[test]
    fn unknown_name_errors() {
        assert_eq!(
            valid("xs:nope", "3").unwrap_err(),
            UnknownBuiltin("xs:nope".to_string())
        );
    }

    #[test]
    fn qname_reservation() {
        assert!(qname("ns:foo"));
        assert!(!qname("xmlns:xsi"));
        assert!(!qname(":foo"));
        assert!(!qname("a:b:c"));
    }

    #[test]
    fn duration_rules() {
        assert!(duration("P1Y2M3DT4H5M6.5S"));
        assert!(duration("PT0S"));
        assert!(duration("-P1D"));
        assert!(!duration("P"));
        assert!(!duration("PT"));
        assert!(!duration("P1Y2MT"));
        assert!(!duration("P200.5Y"));
        assert!(!duration("P1Y1Y"));
    }

    #[test]
    fn calendar_rules() {
        assert!(calendar_date(2024, 2, 29));
        assert!(!calendar_date(1999, 2, 29));
        assert!(!calendar_date(2024, 4, 31));
    }

    #[test]
    fn zone_shapes() {
        assert!(zone("Z"));
        assert!(zone("+05:30"));
        assert!(zone("-01:00"));
        assert!(zone("14:00"));
        assert!(!zone("15:00"));
        assert!(!zone("+25:99"));
        assert!(!zone("01:00"));
    }

    #[test]
    fn negative_dates_pass() {
        // The negative year's own sign must not read as a zone attempt.
        assert!(date_part("-2024-01-01"));
        assert!(!date_part("-2024-02-30"));
    }
}
