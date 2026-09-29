//! Built-in string format validators (spec §5.4).
//!
//! All formats are implemented in pure Rust from scratch — no external
//! format-validation dependencies. Date and date-time parsing use `chrono`
//! (MIT / Apache-2.0) for calendar correctness; email, URI, UUID, IPv4, IPv6
//! and hostname validation are hand-written.

use serde_json::Value;

/// Supported string formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// `email` — practical RFC 5322 subset.
    Email,
    /// `uri` — RFC 3986 absolute URI (`url` is accepted as an alias).
    Uri,
    /// `date` — ISO 8601 full-date (`YYYY-MM-DD`).
    Date,
    /// `date-time` — RFC 3339 date-time.
    DateTime,
    /// `uuid` — RFC 4122 textual UUID (8-4-4-4-12 hex, any case).
    Uuid,
    /// `ipv4` — dotted-quad IPv4 (leading zeros rejected).
    Ipv4,
    /// `ipv6` — RFC 4294 textual IPv6 (incl. `::` compression and IPv4 tails).
    Ipv6,
    /// `hostname` — RFC 1123 internet host name.
    Hostname,
}

impl Format {
    /// Map a JSON Schema `format` keyword to a [`Format`], if supported.
    pub fn from_keyword(keyword: &str) -> Option<Format> {
        match keyword {
            "email" => Some(Format::Email),
            "uri" | "url" | "iri" => Some(Format::Uri),
            "date" => Some(Format::Date),
            "date-time" | "datetime" => Some(Format::DateTime),
            "uuid" => Some(Format::Uuid),
            "ipv4" => Some(Format::Ipv4),
            "ipv6" => Some(Format::Ipv6),
            "hostname" | "idn-hostname" => Some(Format::Hostname),
            _ => None,
        }
    }

    /// The canonical keyword for this format.
    pub fn keyword(&self) -> &'static str {
        match self {
            Format::Email => "email",
            Format::Uri => "uri",
            Format::Date => "date",
            Format::DateTime => "date-time",
            Format::Uuid => "uuid",
            Format::Ipv4 => "ipv4",
            Format::Ipv6 => "ipv6",
            Format::Hostname => "hostname",
        }
    }

    /// Validate a string against this format.
    pub fn is_valid(&self, s: &str) -> bool {
        match self {
            Format::Email => is_valid_email(s),
            Format::Uri => is_valid_uri(s),
            Format::Date => is_valid_date(s),
            Format::DateTime => is_valid_datetime(s),
            Format::Uuid => is_valid_uuid(s),
            Format::Ipv4 => is_valid_ipv4(s),
            Format::Ipv6 => is_valid_ipv6(s),
            Format::Hostname => is_valid_hostname(s),
        }
    }

    /// Validate a JSON value (only strings can carry a format).
    pub fn validate_value(&self, value: &Value) -> bool {
        match value {
            Value::String(s) => self.is_valid(s),
            // Per JSON Schema, format assertions apply only to strings.
            _ => true,
        }
    }
}

fn is_ascii_alnum_or(c: char, extra: &str) -> bool {
    c.is_ascii_alphanumeric() || extra.contains(c)
}

/// Practical RFC 5322 subset (what real-world mail systems accept):
/// `local-part@domain`, dot-atom local part, DNS-style domain.
pub fn is_valid_email(s: &str) -> bool {
    if s.len() > 254 || s.is_empty() {
        return false;
    }
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    if domain.contains('@') {
        return false;
    }

    // Local part: 1–64 chars, dot-atom (no leading/trailing/double dots).
    if local.is_empty() || local.len() > 64 {
        return false;
    }
    if local.starts_with('.') || local.ends_with('.') || local.contains("..") {
        return false;
    }
    if !local
        .chars()
        .all(|c| is_ascii_alnum_or(c, "!#$%&'*+-/=?^_`{|}~."))
    {
        return false;
    }

    is_valid_hostname(domain)
}

/// RFC 3986 absolute-URI shape: `scheme:...`, scheme = `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`.
pub fn is_valid_uri(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() {
        return false;
    }
    let mut scheme_len = 1usize;
    let mut colon_seen = false;
    for c in chars {
        if c == ':' {
            colon_seen = true;
            break;
        }
        if !(c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.') {
            // Non-scheme character before the colon: could still be a valid
            // relative reference, but not an absolute URI.
            return false;
        }
        scheme_len += 1;
        if scheme_len > 64 {
            return false;
        }
    }
    if !colon_seen || scheme_len >= s.len() {
        return false;
    }
    // The remainder must not contain raw control characters or spaces.
    s.chars().all(|c| !c.is_ascii_control() && c != ' ')
}

/// ISO 8601 `YYYY-MM-DD` full-date.
pub fn is_valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    if !(b[..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..10].iter().all(u8::is_ascii_digit))
    {
        return false;
    }
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

/// RFC 3339 date-time, e.g. `1963-06-19T08:30:06.283185Z` or
/// `1963-06-19T08:30:06+01:00`. Lowercase `t`/`z` separators are accepted
/// (RFC 3339 §5.6 NOTE); space separators are not.
pub fn is_valid_datetime(s: &str) -> bool {
    if s.len() < 20 || s.is_empty() {
        return false;
    }
    // RFC 3339 requires the 'T' (or lowercase 't') separator; chrono is
    // lenient about a space, so enforce this ourselves.
    let sep = s.as_bytes()[10];
    if sep != b'T' && sep != b't' {
        return false;
    }
    // Fast path: chrono accepts uppercase T/Z directly; only allocate when a
    // lowercase RFC 3339 marker needs normalizing.
    let needs_normalizing = s.ends_with('z') || sep == b't';
    if !needs_normalizing {
        return chrono::DateTime::parse_from_rfc3339(s).is_ok();
    }
    let mut normalized = s.to_string();
    if normalized.ends_with('z') {
        let len = normalized.len();
        normalized.replace_range(len - 1.., "Z");
    }
    if normalized.len() > 10 && normalized.as_bytes()[10] == b't' {
        normalized.replace_range(10..11, "T");
    }
    chrono::DateTime::parse_from_rfc3339(&normalized).is_ok()
}

/// RFC 4122 textual UUID: 8-4-4-4-12 hex digits, hyphen-separated, any case.
/// The nil UUID `00000000-0000-0000-0000-000000000000` is valid.
pub fn is_valid_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    for (i, byte) in b.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if *byte != b'-' {
                    return false;
                }
            }
            _ => {
                if !byte.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

/// Dotted-quad IPv4. Leading zeros are rejected (ambiguous octal-style
/// notation, matching the JSON Schema format test suite).
pub fn is_valid_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().all(|p| {
        if p.is_empty() || p.len() > 3 || !p.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        if p.len() > 1 && p.starts_with('0') {
            return false;
        }
        p.parse::<u16>().map(|v| v <= 255).unwrap_or(false)
    })
}

/// RFC 4291 textual IPv6 with `::` compression and optional IPv4-mapped tail.
pub fn is_valid_ipv6(s: &str) -> bool {
    if s.is_empty() || s.matches("::").count() > 1 {
        return false;
    }

    // Zone IDs (e.g. fe80::1%eth0) are not accepted by the JSON Schema format.
    if s.contains('%') {
        return false;
    }

    let (head, tail, compressed) = match s.split_once("::") {
        Some((h, t)) => (h, t, true),
        None => (s, "", false),
    };

    let mut groups: Vec<&str> = Vec::new();
    if !head.is_empty() {
        groups.extend(head.split(':'));
    }
    let tail_groups: Vec<&str> = if tail.is_empty() {
        Vec::new()
    } else {
        tail.split(':').collect()
    };
    groups.extend(tail_groups);

    // An IPv4 tail on the last group counts as two 16-bit groups.
    let last = groups.last().copied().unwrap_or("");
    let ends_with_ipv4 = last.contains('.');
    if ends_with_ipv4 {
        if !is_valid_ipv4(last) {
            return false;
        }
        groups.pop();
    }

    if groups.iter().any(|g| g.is_empty()) {
        return false;
    }
    if !groups
        .iter()
        .all(|g| g.len() <= 4 && g.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return false;
    }

    if compressed {
        groups.len() <= 8
    } else {
        groups.len() == 8 && !ends_with_ipv4 || (groups.len() == 6 && ends_with_ipv4)
    }
}

/// RFC 1123 host name: dot-separated labels of letters, digits and hyphens,
/// not starting or ending with a hyphen; total length ≤ 253.
pub fn is_valid_hostname(s: &str) -> bool {
    if s.is_empty() || s.len() > 253 {
        return false;
    }
    if s.starts_with('.') || s.ends_with('.') || s.contains("..") {
        return false;
    }
    for label in s.split('.') {
        if label.is_empty() || label.len() > 63 {
            return false;
        }
        if label.starts_with('-') || label.ends_with('-') {
            return false;
        }
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return false;
        }
    }
    // A single-label host is allowed (e.g. "localhost"); a numeric TLD is not.
    let last = s.rsplit('.').next().unwrap_or("");
    !(last.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_valid_cases() {
        for s in [
            "alice@example.com",
            "a@b",
            "first.last+tag@sub.domain.example",
            "x_1-y@example-domain.co",
            "user!def!xyz%abc@example.com",
        ] {
            assert!(is_valid_email(s), "{s} should be valid");
        }
    }

    #[test]
    fn email_invalid_cases() {
        for s in [
            "",
            "no-at-sign",
            "@example.com",
            "user@",
            "user@@example.com",
            ".dot@example.com",
            "dot.@example.com",
            "do..t@example.com",
            "user@-bad.com",
            "user@bad-.com",
            "user@.example.com",
            "user@example.com.",
            "user@example..com",
            "user@192.168.0.1", // numeric TLD
            "user name@example.com",
            &format!("{}@example.com", "a".repeat(65)),
            &format!("{}@example.com", "a".repeat(200)),
        ] {
            assert!(!is_valid_email(s), "{s} should be invalid");
        }
    }

    #[test]
    fn uri_valid_cases() {
        for s in [
            "https://example.com",
            "http://example.com/path?query=1#frag",
            "mailto:user@example.com",
            "urn:isbn:0451450523",
            "ftp://files.example.org:21/pub",
            "custom+scheme://h",
            "data:text/plain,hello",
        ] {
            assert!(is_valid_uri(s), "{s} should be valid");
        }
    }

    #[test]
    fn uri_invalid_cases() {
        for s in [
            "",
            "example.com",
            "1http://a",
            "://no-scheme",
            "http ://a",
            "https://exa mple.com",
            "https://a\nb",
        ] {
            assert!(!is_valid_uri(s), "{s} should be invalid");
        }
    }

    #[test]
    fn date_cases() {
        for s in ["2026-09-29", "2024-02-29", "0000-01-01", "9999-12-31"] {
            assert!(is_valid_date(s), "{s} should be valid");
        }
        for s in [
            "2025-02-29", // not a leap year
            "2026-13-01",
            "2026-00-10",
            "2026-09-00",
            "2026-09-31",
            "2026-9-29",
            "20260929",
            "2026-09-29T00:00:00Z",
            "",
        ] {
            assert!(!is_valid_date(s), "{s} should be invalid");
        }
    }

    #[test]
    fn datetime_cases() {
        for s in [
            "1963-06-19T08:30:06Z",
            "1963-06-19t08:30:06.283185z", // lowercase per RFC 3339
            "2026-09-29T00:00:00+05:00",
            "2026-09-29T23:59:59.999999-08:00",
            "2026-02-29T12:00:00Z", // leap day 2026? no — invalid
            "2000-02-29T12:00:00Z",
        ] {
            let expect = s != "2026-02-29T12:00:00Z";
            assert_eq!(is_valid_datetime(s), expect, "{s}");
        }
        for s in [
            "",
            "1963-06-19T08:30:06",  // no offset
            "1963-06-19 08:30:06Z", // space not RFC 3339 — rejected here
            "1963-06-19T25:00:00Z",
            "1963-06-19T08:60:00Z",
        ] {
            assert!(!is_valid_datetime(s), "{s} should be invalid");
        }
    }

    #[test]
    fn uuid_cases() {
        assert!(is_valid_uuid("123e4567-e89b-12d3-a456-426614174000"));
        assert!(is_valid_uuid("123E4567-E89B-12D3-A456-426614174000"));
        assert!(is_valid_uuid("00000000-0000-0000-0000-000000000000"));
        for s in [
            "",
            "123e4567e89b12d3a456426614174000",    // no hyphens
            "123e4567-e89b-12d3-a456-42661417400", // too short
            "123e4567_e89b-12d3-a456-426614174000",
            "g23e4567-e89b-12d3-a456-426614174000",
            "123e4567-e89b-12d3-a456-4266141740000",
        ] {
            assert!(!is_valid_uuid(s), "{s} should be invalid");
        }
    }

    #[test]
    fn ipv4_cases() {
        for s in ["192.168.0.1", "0.0.0.0", "255.255.255.255", "127.0.0.1"] {
            assert!(is_valid_ipv4(s), "{s} should be valid");
        }
        for s in [
            "256.0.0.1",
            "192.168.0",
            "192.168.0.1.2",
            "192.168.01.1", // leading zero
            "192.168..1",
            "a.b.c.d",
            "",
            "-1.0.0.0",
        ] {
            assert!(!is_valid_ipv4(s), "{s} should be invalid");
        }
    }

    #[test]
    fn ipv6_cases() {
        for s in [
            "::1",
            "::",
            "2001:db8::8a2e:370:7334",
            "fe80::1",
            "::ffff:192.168.0.1",
            "2001:0db8:0000:0000:0000:0000:0000:0001",
            "2001:db8:85a3:0:0:8a2e:370:7334",
            "::ffff:0:0",
            "0:0:0:0:0:0:0:0",
        ] {
            assert!(is_valid_ipv6(s), "{s} should be valid");
        }
        for s in [
            "",
            "1:2:3:4:5:6:7:8:9", // 9 groups
            "2001::85a3::7334",  // double compression
            ":2001:db8",
            "2001:db8:",
            "2001:db8:::1",
            "12345::", // group too long
            "g001::1",
            "::ffff:192.168.0.256", // bad ipv4 tail
            "fe80::1%eth0",         // zone id
            "1:2:3:4:5:6:7",        // 7 groups, no compression
        ] {
            assert!(!is_valid_ipv6(s), "{s} should be invalid");
        }
    }

    #[test]
    fn hostname_cases() {
        for s in [
            "example.com",
            "localhost",
            "a-b.example",
            "xn--nxasmq6b",
            "a",
            "0host",
        ] {
            assert!(is_valid_hostname(s), "{s} should be valid");
        }
        for s in [
            "",
            "-example.com",
            "example.com-",
            "example..com",
            "under_score.com",
            &format!("{}x", "a".repeat(253)),
            "1.2.3.4", // all-numeric TLD
        ] {
            assert!(!is_valid_hostname(s), "{s} should be invalid");
        }
    }

    #[test]
    fn keyword_mapping() {
        assert_eq!(Format::from_keyword("email"), Some(Format::Email));
        assert_eq!(Format::from_keyword("url"), Some(Format::Uri));
        assert_eq!(Format::from_keyword("datetime"), Some(Format::DateTime));
        assert!(Format::from_keyword("color").is_none());
    }

    #[test]
    fn value_validation_only_applies_to_strings() {
        assert!(Format::Email.validate_value(&serde_json::json!(42)));
        assert!(!Format::Email.validate_value(&serde_json::json!("not-an-email")));
    }
}
