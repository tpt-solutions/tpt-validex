//! Minimal glob matching (no external dependencies): `*` matches any run of
//! characters within a path component, `**` matches across components, `?`
//! matches one character. Matching is component-based on the normalized
//! forward-slash form so it behaves identically on Windows and Unix.

/// Whether `path` matches `pattern`.
pub(crate) fn matches(pattern: &str, path: &str) -> bool {
    let normalized_pattern = pattern.replace('\\', "/");
    let normalized_path = path.replace('\\', "/");
    let pattern: Vec<&str> = normalized_pattern.split('/').collect();
    let path: Vec<&str> = normalized_path.split('/').collect();
    match_components(&pattern, &path)
}

fn match_components(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => {
            // `**` consumes zero or more path components.
            (0..=path.len()).any(|skip| match_components(rest, &path[skip..]))
        }
        Some((&first, rest)) => {
            let Some((head, tail)) = path.split_first() else {
                return false;
            };
            match_component(first, head) && match_components(rest, tail)
        }
    }
}

/// Match one path component with `*` and `?` wildcards.
fn match_component(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    glob_chars(&p, &t)
}

fn glob_chars(p: &[char], t: &[char]) -> bool {
    match (p.first(), t.first()) {
        (None, None) => true,
        (Some('*'), _) => {
            for skip in 0..=t.len() {
                if glob_chars(&p[1..], &t[skip..]) {
                    return true;
                }
            }
            false
        }
        (Some('?'), Some(_)) => glob_chars(&p[1..], &t[1..]),
        (Some(&pc), Some(&tc)) if pc == tc => glob_chars(&p[1..], &t[1..]),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_star() {
        assert!(matches("data/*.json", "data/users.json"));
        assert!(!matches("data/*.json", "data/nested/users.json"));
        assert!(matches("*.json", "x.json"));
    }

    #[test]
    fn double_star() {
        assert!(matches("**/*.json", "a/b/c.json"));
        assert!(matches("data/**/*.json", "data/c.json"));
        assert!(!matches("data/**/*.json", "other/c.json"));
    }

    #[test]
    fn question_mark() {
        assert!(matches("file?.json", "file1.json"));
        assert!(!matches("file?.json", "file10.json"));
    }

    #[test]
    fn windows_separators_normalized() {
        assert!(matches("data/*.json", r"data\users.json"));
    }
}
