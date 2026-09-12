//! Alpine (apk) version ordering, per apk-tools' `apk_version_compare`.

use core::cmp::Ordering;

/// Rank of the bare release. `_alpha`..`_rc` sort below it, `_cvs`..`_p` above.
const RELEASE_RANK: u8 = 4;

fn suffix_rank(name: &str) -> Option<u8> {
    Some(match name {
        "alpha" => 0,
        "beta" => 1,
        "pre" => 2,
        "rc" => 3,
        "cvs" => 5,
        "svn" => 6,
        "git" => 7,
        "hg" => 8,
        "p" => 9,
        _ => return None,
    })
}

/// An apk version split into its comparable fields.
#[derive(Debug, PartialEq, Eq)]
struct Parsed {
    numbers: Vec<u64>,
    letter: Option<u8>,
    suffix: u8,
    suffix_number: u64,
    revision: u64,
}

/// Compare two apk version strings: `digits[.digits]*[letter][_suffix[n]][-rN]`.
pub fn compare(left: &str, right: &str) -> Ordering {
    let (left, right) = (parse(left), parse(right));

    // Zero-fill the shorter list so 1.36 sorts below 1.36.1.
    let width = left.numbers.len().max(right.numbers.len());
    for index in 0..width {
        let a = left.numbers.get(index).copied().unwrap_or(0);
        let b = right.numbers.get(index).copied().unwrap_or(0);
        if a != b {
            return a.cmp(&b);
        }
    }

    left.letter
        .cmp(&right.letter)
        .then_with(|| left.suffix.cmp(&right.suffix))
        .then_with(|| left.suffix_number.cmp(&right.suffix_number))
        .then_with(|| left.revision.cmp(&right.revision))
}

fn parse(version: &str) -> Parsed {
    let bytes = version.as_bytes();
    let mut index = 0;
    let mut numbers = Vec::new();

    loop {
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == start {
            break;
        }
        numbers.push(version[start..index].parse().unwrap_or(0));
        if index < bytes.len() && bytes[index] == b'.' {
            index += 1;
        } else {
            break;
        }
    }

    let letter = match bytes.get(index) {
        Some(byte) if byte.is_ascii_alphabetic() => {
            index += 1;
            Some(*byte)
        }
        _ => None,
    };

    let mut suffix = RELEASE_RANK;
    let mut suffix_number = 0;
    if bytes.get(index) == Some(&b'_') {
        let start = index + 1;
        let mut end = start;
        while end < bytes.len() && bytes[end].is_ascii_alphabetic() {
            end += 1;
        }
        if let Some(rank) = suffix_rank(&version[start..end]) {
            suffix = rank;
            index = end;
            let digits = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            suffix_number = version[digits..index].parse().unwrap_or(0);
        }
    }

    // An absent revision reads as r0, so 1.36.1 sorts below 1.36.1-r1.
    let revision = version
        .rfind("-r")
        .and_then(|at| version[at + 2..].parse().ok())
        .unwrap_or(0);

    Parsed {
        numbers,
        letter,
        suffix,
        suffix_number,
        revision,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cmp::Ordering::*;

    fn cmp(a: &str, b: &str) -> Ordering {
        compare(a, b)
    }

    #[test]
    fn equal_versions_compare_equal() {
        assert_eq!(cmp("1.36.1-r5", "1.36.1-r5"), Equal);
    }

    #[test]
    fn the_revision_breaks_a_tie() {
        assert_eq!(cmp("1.36.1-r4", "1.36.1-r5"), Less);
        assert_eq!(cmp("1.36.1", "1.36.1-r1"), Less);
    }

    #[test]
    fn numeric_parts_compare_as_numbers() {
        assert_eq!(cmp("1.36.1", "1.36.2"), Less);
        assert_eq!(cmp("1.10", "1.9"), Greater);
    }

    #[test]
    fn more_parts_outrank_fewer_when_the_prefix_matches() {
        assert_eq!(cmp("1.36", "1.36.1"), Less);
    }

    #[test]
    fn a_trailing_letter_outranks_the_bare_version() {
        assert_eq!(cmp("1.0", "1.0a"), Less);
        assert_eq!(cmp("1.0a", "1.0b"), Less);
    }

    #[test]
    fn pre_release_suffixes_sort_below_the_release() {
        assert_eq!(cmp("1.0_alpha1", "1.0_beta1"), Less);
        assert_eq!(cmp("1.0_beta1", "1.0_pre1"), Less);
        assert_eq!(cmp("1.0_pre1", "1.0_rc1"), Less);
        assert_eq!(cmp("1.0_rc1", "1.0"), Less);
    }

    #[test]
    fn post_release_suffixes_sort_above_the_release() {
        assert_eq!(cmp("1.0", "1.0_p1"), Less);
        assert_eq!(cmp("1.0_p1", "1.0_p2"), Less);
    }
}
