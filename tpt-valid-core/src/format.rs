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
    /// `time` — RFC 3339 full-time (`HH:MM:SS(.frac)?(Z|±HH:MM)`).
    Time,
    /// `duration` — ISO 8601 duration (`PnYnMnDTnHnMnS`, `PnW`).
    Duration,
    /// `uuid` — RFC 4122 textual UUID (8-4-4-4-12 hex, any case).
    Uuid,
    /// `ipv4` — dotted-quad IPv4 (leading zeros rejected).
    Ipv4,
    /// `ipv6` — RFC 4294 textual IPv6 (incl. `::` compression and IPv4 tails).
    Ipv6,
    /// `hostname` — RFC 1123 internet host name.
    Hostname,
    /// `phone` — E.164-style international telephone number.
    Phone,
    /// `currency` — ISO 4217 alphabetic currency code (3 uppercase letters).
    Currency,
    /// `iban` — ISO 13616 IBAN with mod-97 checksum validation.
    Iban,
    /// `country-code` — ISO 3166-1 alpha-2 country code.
    CountryCode,
    /// `semver` — Semantic Versioning 2.0.0.
    Semver,
    /// `regex` — a valid regular expression (ECMA-262 flavor intended;
    /// validated with the `regex` crate syntax).
    Regex,
    /// `json-pointer` — RFC 6901 JSON Pointer.
    JsonPointer,
}

/// The `format` keyword → [`Format`] mapping, including documented aliases.
pub fn format_keyword_map() -> &'static [(&'static str, Format)] {
    &[
        ("email", Format::Email),
        ("uri", Format::Uri),
        ("url", Format::Uri),
        ("iri", Format::Uri),
        ("date", Format::Date),
        ("date-time", Format::DateTime),
        ("datetime", Format::DateTime),
        ("time", Format::Time),
        ("duration", Format::Duration),
        ("uuid", Format::Uuid),
        ("ipv4", Format::Ipv4),
        ("ipv6", Format::Ipv6),
        ("hostname", Format::Hostname),
        ("idn-hostname", Format::Hostname),
        ("phone", Format::Phone),
        ("currency", Format::Currency),
        ("iban", Format::Iban),
        ("country-code", Format::CountryCode),
        ("semver", Format::Semver),
        ("regex", Format::Regex),
        ("json-pointer", Format::JsonPointer),
    ]
}

impl Format {
    /// Map a JSON Schema `format` keyword to a [`Format`], if supported.
    pub fn from_keyword(keyword: &str) -> Option<Format> {
        format_keyword_map()
            .iter()
            .find(|(k, _)| *k == keyword)
            .map(|(_, f)| *f)
    }

    /// The canonical keyword for this format.
    pub fn keyword(&self) -> &'static str {
        match self {
            Format::Email => "email",
            Format::Uri => "uri",
            Format::Date => "date",
            Format::DateTime => "date-time",
            Format::Time => "time",
            Format::Duration => "duration",
            Format::Uuid => "uuid",
            Format::Ipv4 => "ipv4",
            Format::Ipv6 => "ipv6",
            Format::Hostname => "hostname",
            Format::Phone => "phone",
            Format::Currency => "currency",
            Format::Iban => "iban",
            Format::CountryCode => "country-code",
            Format::Semver => "semver",
            Format::Regex => "regex",
            Format::JsonPointer => "json-pointer",
        }
    }

    /// Validate a string against this format.
    pub fn is_valid(&self, s: &str) -> bool {
        match self {
            Format::Email => is_valid_email(s),
            Format::Uri => is_valid_uri(s),
            Format::Date => is_valid_date(s),
            Format::DateTime => is_valid_datetime(s),
            Format::Time => is_valid_time(s),
            Format::Duration => is_valid_duration(s),
            Format::Uuid => is_valid_uuid(s),
            Format::Ipv4 => is_valid_ipv4(s),
            Format::Ipv6 => is_valid_ipv6(s),
            Format::Hostname => is_valid_hostname(s),
            Format::Phone => is_valid_phone(s),
            Format::Currency => is_valid_currency(s),
            Format::Iban => is_valid_iban(s),
            Format::CountryCode => is_valid_country_code(s),
            Format::Semver => is_valid_semver(s),
            Format::Regex => regex::Regex::new(s).is_ok(),
            Format::JsonPointer => is_valid_json_pointer(s),
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
    let first = chars
        .next()
        .expect("s is non-empty (checked at function entry)");
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

/// E.164-style international telephone number: `+` country code and up to 15
/// digits. Spaces, dashes, dots and parentheses are tolerated as formatting
/// (stripped before the digit check); the bare `+` prefix is required.
pub fn is_valid_phone(s: &str) -> bool {
    let cleaned: String = s
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '.' | '(' | ')'))
        .collect();
    let Some(rest) = cleaned.strip_prefix('+') else {
        return false;
    };
    let digits = rest.chars().count();
    (8..=15).contains(&digits) && rest.chars().all(|c| c.is_ascii_digit())
}

/// ISO 4217 alphabetic currency code: three uppercase ASCII letters from the
/// official list.
pub fn is_valid_currency(s: &str) -> bool {
    const CURRENCIES: &[&str] = &[
        "AED", "AFN", "ALL", "AMD", "ANG", "AOA", "ARS", "AUD", "AWG", "AZN", "BAM", "BBD", "BDT",
        "BGN", "BHD", "BIF", "BMD", "BND", "BOB", "BRL", "BSD", "BTN", "BWP", "BYN", "BZD", "CAD",
        "CDF", "CHF", "CLP", "CNY", "COP", "CRC", "CUP", "CVE", "CZK", "DJF", "DKK", "DOP", "DZD",
        "EGP", "ERN", "ETB", "EUR", "FJD", "FKP", "GBP", "GEL", "GHS", "GIP", "GMD", "GNF", "GTQ",
        "GYD", "HKD", "HNL", "HTG", "HUF", "IDR", "ILS", "INR", "IQD", "IRR", "ISK", "JMD", "JOD",
        "JPY", "KES", "KGS", "KHR", "KMF", "KPW", "KRW", "KWD", "KYD", "KZT", "LAK", "LBP", "LKR",
        "LRD", "LSL", "LYD", "MAD", "MDL", "MGA", "MKD", "MMK", "MNT", "MOP", "MRU", "MUR", "MVR",
        "MWK", "MXN", "MYR", "MZN", "NAD", "NGN", "NIO", "NOK", "NPR", "NZD", "OMR", "PAB", "PEN",
        "PGK", "PHP", "PKR", "PLN", "PYG", "QAR", "RON", "RSD", "RUB", "RWF", "SAR", "SBD", "SCR",
        "SDG", "SEK", "SGD", "SHP", "SLE", "SOS", "SRD", "SSP", "STN", "SVC", "SYP", "SZL", "THB",
        "TJS", "TMT", "TND", "TOP", "TRY", "TTD", "TWD", "TZS", "UAH", "UGX", "USD", "UYU", "UZS",
        "VED", "VES", "VND", "VUV", "WST", "XAF", "XCD", "XOF", "XPF", "YER", "ZAR", "ZMW", "ZWG",
        // Funds and special codes.
        "XAU", "XAG", "XPT", "XPD", "XDR", "XSU", "XUA", "BOV", "CHE", "CHW", "CLF", "COU", "CUC",
        "MXV", "USN", "UYW",
    ];
    CURRENCIES.contains(&s)
}

/// ISO 13616 IBAN: country code + two check digits + BBAN (15–34 characters
/// total), validated with the standard mod-97 check.
pub fn is_valid_iban(s: &str) -> bool {
    let compact: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() < 15 || compact.len() > 34 {
        return false;
    }
    let bytes = compact.as_bytes();
    if !bytes[0..2].iter().all(u8::is_ascii_uppercase) {
        return false;
    }
    if !bytes[2..4].iter().all(u8::is_ascii_digit) {
        return false;
    }
    if !bytes[4..].iter().all(|b| b.is_ascii_alphanumeric()) {
        return false;
    }
    // Move the first four characters to the end, map letters to numbers
    // (A=10 … Z=35) and run the ISO 7064 mod-97-10 check.
    let rearranged: String = compact[4..].to_string() + &compact[..4];
    let mut remainder: u32 = 0;
    for c in rearranged.chars() {
        if c.is_ascii_digit() {
            remainder = (remainder * 10 + u32::from(c as u8 - b'0')) % 97;
        } else {
            // Letters expand to two digits (10–35).
            let n = u32::from(c.to_ascii_uppercase() as u8 - b'A') + 10;
            remainder = (remainder * 100 + n) % 97;
        }
    }
    remainder == 1
}

/// ISO 3166-1 alpha-2 country code (officially assigned).
pub fn is_valid_country_code(s: &str) -> bool {
    const COUNTRIES: &[&str] = &[
        "AD", "AE", "AF", "AG", "AI", "AL", "AM", "AO", "AQ", "AR", "AS", "AT", "AU", "AW", "AX",
        "AZ", "BA", "BB", "BD", "BE", "BF", "BG", "BH", "BI", "BJ", "BL", "BM", "BN", "BO", "BQ",
        "BR", "BS", "BT", "BV", "BW", "BY", "BZ", "CA", "CC", "CD", "CF", "CG", "CH", "CI", "CK",
        "CL", "CM", "CN", "CO", "CR", "CU", "CV", "CW", "CX", "CY", "CZ", "DE", "DJ", "DK", "DM",
        "DO", "DZ", "EC", "EE", "EG", "EH", "ER", "ES", "ET", "FI", "FJ", "FK", "FM", "FO", "FR",
        "GA", "GB", "GD", "GE", "GF", "GG", "GH", "GI", "GL", "GM", "GN", "GP", "GQ", "GR", "GS",
        "GT", "GU", "GW", "GY", "HK", "HM", "HN", "HR", "HT", "HU", "ID", "IE", "IL", "IM", "IN",
        "IO", "IQ", "IR", "IS", "IT", "JE", "JM", "JO", "JP", "KE", "KG", "KH", "KI", "KM", "KN",
        "KP", "KR", "KW", "KY", "KZ", "LA", "LB", "LC", "LI", "LK", "LR", "LS", "LT", "LU", "LV",
        "LY", "MA", "MC", "MD", "ME", "MF", "MG", "MH", "MK", "ML", "MM", "MN", "MO", "MP", "MQ",
        "MR", "MS", "MT", "MU", "MV", "MW", "MX", "MY", "MZ", "NA", "NC", "NE", "NF", "NG", "NI",
        "NL", "NO", "NP", "NR", "NU", "NZ", "OM", "PA", "PE", "PF", "PG", "PH", "PK", "PL", "PM",
        "PN", "PR", "PS", "PT", "PW", "PY", "QA", "RE", "RO", "RS", "RU", "RW", "SA", "SB", "SC",
        "SD", "SE", "SG", "SH", "SI", "SJ", "SK", "SL", "SM", "SN", "SO", "SR", "SS", "ST", "SV",
        "SX", "SY", "SZ", "TC", "TD", "TF", "TG", "TH", "TJ", "TK", "TL", "TM", "TN", "TO", "TR",
        "TT", "TV", "TW", "TZ", "UA", "UG", "UM", "US", "UY", "UZ", "VA", "VC", "VE", "VG", "VI",
        "VN", "VU", "WF", "WS", "YE", "YT", "ZA", "ZM", "ZW",
    ];
    COUNTRIES.contains(&s)
}

/// Semantic Versioning 2.0.0: `MAJOR.MINOR.PATCH(-prerelease)?(+build)?`.
pub fn is_valid_semver(s: &str) -> bool {
    let (version, build) = match s.split_once('+') {
        Some((v, b)) => (v, Some(b)),
        None => (s, None),
    };
    if let Some(b) = build {
        // Build metadata: dot-separated non-empty alphanumeric+hyphen ids.
        if b.split('.').any(|id| id.is_empty() || !id.chars().all(is_build_char)) {
            return false;
        }
    }
    let (core, prerelease) = match version.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (version, None),
    };
    let core_parts: Vec<&str> = core.split('.').collect();
    if core_parts.len() != 3 {
        return false;
    }
    // No leading zeros (except "0" itself) in the numeric core.
    if core_parts
        .iter()
        .any(|p| !is_numeric_identifier(p) || (p.len() > 1 && p.starts_with('0')))
    {
        return false;
    }
    if let Some(pre) = prerelease {
        for id in pre.split('.') {
            if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                return false;
            }
            // Numeric identifiers must not have leading zeros.
            if id.chars().all(|c| c.is_ascii_digit())
                && id.len() > 1
                && id.starts_with('0')
            {
                return false;
            }
        }
    }
    true
}

fn is_build_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}

/// RFC 6901 JSON Pointer: empty string, or `/`-separated tokens with `~0`/`~1`
/// escapes (a bare `~` or `~2+` is invalid).
pub fn is_valid_json_pointer(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    if !s.starts_with('/') {
        return false;
    }
    for token in s.split('/').skip(1) {
        let mut chars = token.chars();
        while let Some(c) = chars.next() {
            if c != '~' {
                continue;
            }
            match chars.next() {
                Some('0') | Some('1') => {}
                _ => return false,
            }
        }
    }
    true
}

fn is_numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// ISO 8601 duration: `PnYnMnDTnHnMnS`, `PnW`, or with fractional seconds.
pub fn is_valid_duration(s: &str) -> bool {
    let Some(rest) = s.strip_prefix('P') else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    // Weeks form.
    if let Some(weeks) = rest.strip_suffix('W') {
        return is_positive_number(weeks);
    }
    let (date_part, time_part) = match rest.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (rest, None),
    };

    let mut seen = [false; 3]; // Y, M, D — order enforced
    let mut last_unit = 0usize;
    let date_ok = if date_part.is_empty() && time_part.is_some() {
        true // "PT…" — all components in the time part
    } else {
        parse_duration_components(date_part, |unit| {
            let order = match unit {
                'Y' => 0,
                'M' => 1,
                'D' => 2,
                _ => return false,
            };
            if order < last_unit || seen[order] {
                return false;
            }
            seen[order] = true;
            last_unit = order;
            true
        })
    };
    if !date_ok {
        return false;
    }

    if let Some(t) = time_part {
        let mut seen_t = [false; 3]; // H, M, S
        let mut last_t = 0usize;
        if !parse_duration_components(t, |unit| {
            let order = match unit {
                'H' => 0,
                'M' => 1,
                'S' => 2,
                _ => return false,
            };
            if order < last_t || seen_t[order] {
                return false;
            }
            seen_t[order] = true;
            last_t = order;
            true
        }) {
            return false;
        }
    } else if seen.iter().all(|s| !s) {
        return false; // "P" with nothing at all
    }
    true
}

/// Parse `[n]U[n]U...` duration components, calling `unit` per unit letter.
fn parse_duration_components(s: &str, mut unit: impl FnMut(char) -> bool) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut num = String::new();
    let mut any = false;
    for c in s.chars() {
        if c.is_ascii_digit() || c == '.' || c == ',' {
            num.push(c);
            continue;
        }
        if !is_positive_number(&num) {
            return false;
        }
        num.clear();
        any = true;
        if !unit(c) {
            return false;
        }
    }
    // Trailing digits without a unit letter are invalid.
    any && num.is_empty()
}

fn is_positive_number(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',')
        && s.chars().any(|c| c.is_ascii_digit())
}

/// RFC 3339 full-time: `HH:MM:SS(.fraction)?(Z|±HH:MM)` (leap second 60
/// allowed).
pub fn is_valid_time(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() < 8 {
        return false;
    }
    if !(bytes[2] == b':' && bytes[5] == b':') {
        return false;
    }
    if !digits_at(bytes, 0, 2) || !digits_at(bytes, 3, 2) || !digits_at(bytes, 6, 2) {
        return false;
    }
    let hh: u32 = s[0..2].parse().unwrap_or(99);
    let mm: u32 = s[3..5].parse().unwrap_or(99);
    let ss: u32 = s[6..8].parse().unwrap_or(99);
    if hh > 23 || mm > 59 || ss > 60 {
        return false;
    }
    let mut rest = &s[8..];
    if let Some(stripped) = rest.strip_prefix('.') {
        let frac_len = stripped.chars().take_while(|c| c.is_ascii_digit()).count();
        if frac_len == 0 {
            return false;
        }
        rest = &stripped[frac_len..];
    }
    if rest == "Z" || rest == "z" {
        return true;
    }
    if !(rest.len() == 6
        && (rest.starts_with('+') || rest.starts_with('-'))
        && digits_at(rest.as_bytes(), 1, 2)
        && rest.as_bytes()[3] == b':'
        && digits_at(rest.as_bytes(), 4, 2))
    {
        return false;
    }
    let ohh: u32 = rest[1..3].parse().unwrap_or(99);
    let omm: u32 = rest[4..6].parse().unwrap_or(99);
    ohh <= 23 && omm <= 59
}

fn digits_at(b: &[u8], start: usize, count: usize) -> bool {
    b[start..start + count].iter().all(u8::is_ascii_digit)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_keyword_roundtrip() {
        for (kw, f) in format_keyword_map() {
            assert_eq!(Format::from_keyword(kw), Some(*f), "keyword {kw}");
            assert_eq!(f.keyword(), if *kw == "url" || *kw == "iri" {
                "uri"
            } else if *kw == "datetime" {
                "date-time"
            } else if *kw == "idn-hostname" {
                "hostname"
            } else {
                kw
            });
        }
        assert_eq!(Format::from_keyword("nope"), None);
    }
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

    #[test]
    fn time_format() {
        for s in [
            "00:00:00Z",
            "23:59:60Z",       // leap second
            "12:34:56.789+05:30",
            "01:02:03-08:00",
            "12:00:00z",
        ] {
            assert!(is_valid_time(s), "valid: {s}");
        }
        for s in [
            "",
            "12:34:56",
            "24:00:00Z",
            "12:60:00Z",
            "12:34:5Z",
            "12:34:56.",
            "12:34:56+05:70",
            "12-34-56Z",
            "1234567Z",
        ] {
            assert!(!is_valid_time(s), "invalid: {s}");
        }
        assert_eq!(Format::from_keyword("time"), Some(Format::Time));
        assert!(Format::Time.is_valid("10:20:30Z"));
    }

    #[test]
    fn duration_format() {
        for s in [
            "P1Y",
            "P3M",
            "P10D",
            "PT6H",
            "P1Y2M3DT4H5M6S",
            "P1W",
            "PT0.5S",
            "P2W",
            "P1Y6M15D",
        ] {
            assert!(is_valid_duration(s), "valid: {s}");
        }
        for s in [
            "",
            "P",
            "PT",
            "1Y",
            "P1D2M", // order violation
            "P1Q",
            "PW",
            "P1",
        ] {
            assert!(!is_valid_duration(s), "invalid: {s}");
        }
    }

    #[test]
    fn phone_format() {
        for s in [
            "+14155550123",
            "+41 55 550 01 23",
            "+41-55-550-0123",
            "+44 (0) 20 7946 0958".replace("(0) ", "").as_str(),
            "+4155550123",
        ] {
            assert!(is_valid_phone(s), "valid: {s}");
        }
        for s in ["", "4155550123", "+", "+1234", "+12345678901234567890", "abc"] {
            assert!(!is_valid_phone(s), "invalid: {s}");
        }
    }

    #[test]
    fn currency_format() {
        assert!(is_valid_currency("USD"));
        assert!(is_valid_currency("EUR"));
        assert!(is_valid_currency("CHF"));
        assert!(!is_valid_currency("usd"));
        assert!(!is_valid_currency("US"));
        assert!(!is_valid_currency("USDD"));
        assert!(!is_valid_currency("XYZ"));
    }

    #[test]
    fn iban_format() {
        // Officially valid example IBANs (mod-97 correct).
        for s in [
            "GB82 WEST 1234 5698 7654 32",
            "DE89 3704 0044 0532 0130 00",
            "CH93 0076 2011 6238 5295 7",
            "NL91ABNA0417164300",
        ] {
            assert!(is_valid_iban(s), "valid: {s}");
        }
        for s in [
            "",
            "GB82 WEST 1234 5698 7654 33", // bad check digits
            "US82 WEST 1234 5698 7654 32", // not an IBAN country
            "GB10 WEST 1234 5698 7654 32", // wrong check digits
            "DExx 3704 0044 0532 0130 00",
            "GB8", // too short
        ] {
            assert!(!is_valid_iban(s), "invalid: {s}");
        }
    }

    #[test]
    fn country_code_format() {
        assert!(is_valid_country_code("CH"));
        assert!(is_valid_country_code("US"));
        assert!(is_valid_country_code("DE"));
        assert!(!is_valid_country_code("ch"));
        assert!(!is_valid_country_code("XX"));
        assert!(!is_valid_country_code("U"));
        assert!(!is_valid_country_code("USA"));
    }

    #[test]
    fn semver_format() {
        for s in [
            "1.0.0",
            "0.0.1",
            "10.20.30",
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta.1",
            "1.0.0+build.1",
            "1.0.0-rc.1+build.2",
            "1.0.0-x.7.z.92",
        ] {
            assert!(is_valid_semver(s), "valid: {s}");
        }
        for s in [
            "",
            "1",
            "1.0",
            "1.0.0.0",
            "01.0.0",
            "1.0",
            "v1.0.0",
            "1.0.0-",
            "1.0.0-01",
            "1.0.0+",
            "1.0.0+build..2",
            "1.0.0-alpha..1",
        ] {
            assert!(!is_valid_semver(s), "invalid: {s}");
        }
    }

    #[test]
    fn regex_format() {
        assert!(Format::Regex.is_valid("^[a-z]+$"));
        assert!(Format::Regex.is_valid("\\d{4}"));
        assert!(!Format::Regex.is_valid("[unclosed"));
        assert!(!Format::Regex.is_valid("*invalid"));
    }

    #[test]
    fn json_pointer_format() {
        for s in ["", "/foo", "/foo/bar", "/foo/0", "/a~1b", "/c~0d", "/"] {
            assert!(is_valid_json_pointer(s), "valid: {s}");
        }
        for s in ["foo", "a/b", "/a~", "/a~2b", "/a~~"] {
            assert!(!is_valid_json_pointer(s), "invalid: {s}");
        }
    }
}
