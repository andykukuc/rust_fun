//! Debian version ordering, per `deb-version(7)` and dpkg's `verrevcmp`.

use core::cmp::Ordering;

/// Compare two Debian version strings: `[epoch:]upstream[-revision]`.
pub fn compare(left: &str, right: &str) -> Ordering {
    let (left_epoch, left_upstream, left_revision) = split(left);
    let (right_epoch, right_upstream, right_revision) = split(right);

    left_epoch
        .cmp(&right_epoch)
        .then_with(|| verrevcmp(left_upstream.as_bytes(), right_upstream.as_bytes()))
        .then_with(|| verrevcmp(left_revision.as_bytes(), right_revision.as_bytes()))
}

/// Split `[epoch:]upstream[-revision]`.
///
/// An absent epoch reads as 0, so `1.0` and `0:1.0` are the same version.
/// The revision is taken from the LAST hyphen, because upstream versions
/// are allowed to contain hyphens of their own.
fn split(version: &str) -> (u64, &str, &str) {
    let (epoch, rest) = match version.find(':') {
        Some(colon) if colon > 0 && version[..colon].bytes().all(|b| b.is_ascii_digit()) => {
            (version[..colon].parse().unwrap_or(0), &version[colon + 1..])
        }
        _ => (0, version),
    };
    match rest.rfind('-') {
        Some(hyphen) => (epoch, &rest[..hyphen], &rest[hyphen + 1..]),
        None => (epoch, rest, ""),
    }
}

/// Rank one character for the non-digit comparison.
///
/// The ordering that makes `~` a pre-release marker: tilde sorts below the
/// end of the string, which sorts below letters, which sort below every
/// other character.
fn order(character: Option<u8>) -> i32 {
    match character {
        None => 0,
        Some(b) if b.is_ascii_digit() => 0,
        Some(b) if b.is_ascii_alphabetic() => b as i32,
        Some(b'~') => -1,
        Some(b) => b as i32 + 256,
    }
}

/// dpkg's `verrevcmp`: alternating runs of non-digits and digits, with
/// digit runs compared numerically so `1.10` outranks `1.9`.
fn verrevcmp(left: &[u8], right: &[u8]) -> Ordering {
    let (mut i, mut j) = (0usize, 0usize);

    while i < left.len() || j < right.len() {
        let mut first_diff = 0i32;

        while (i < left.len() && !left[i].is_ascii_digit())
            || (j < right.len() && !right[j].is_ascii_digit())
        {
            let left_rank = order(left.get(i).copied());
            let right_rank = order(right.get(j).copied());
            if left_rank != right_rank {
                return left_rank.cmp(&right_rank);
            }
            i += 1;
            j += 1;
        }

        // Leading zeros carry no value: 1.01 and 1.1 are one version.
        while i < left.len() && left[i] == b'0' {
            i += 1;
        }
        while j < right.len() && right[j] == b'0' {
            j += 1;
        }

        while i < left.len()
            && left[i].is_ascii_digit()
            && j < right.len()
            && right[j].is_ascii_digit()
        {
            if first_diff == 0 {
                first_diff = left[i] as i32 - right[j] as i32;
            }
            i += 1;
            j += 1;
        }

        // A longer run of digits is the larger number.
        if i < left.len() && left[i].is_ascii_digit() {
            return Ordering::Greater;
        }
        if j < right.len() && right[j].is_ascii_digit() {
            return Ordering::Less;
        }
        if first_diff != 0 {
            return first_diff.cmp(&0);
        }
    }

    Ordering::Equal
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
        assert_eq!(cmp("1.0", "1.0"), Equal);
        assert_eq!(cmp("1.1.1n-0+deb11u3", "1.1.1n-0+deb11u3"), Equal);
    }

    #[test]
    fn numeric_parts_compare_as_numbers_not_text() {
        // The classic trap: "1.9" is text-greater but version-lesser than "1.10".
        assert_eq!(cmp("1.10", "1.9"), Greater);
        assert_eq!(cmp("1.0", "1.1"), Less);
        assert_eq!(cmp("2.0", "10.0"), Less);
    }

    #[test]
    fn leading_zeros_in_a_numeric_part_are_insignificant() {
        assert_eq!(cmp("1.01", "1.1"), Equal);
        assert_eq!(cmp("1.007", "1.7"), Equal);
    }

    #[test]
    fn an_epoch_outranks_the_upstream_version() {
        assert_eq!(cmp("1:1.0", "2.0"), Greater);
        assert_eq!(cmp("1:1.0", "2:1.0"), Less);
    }

    #[test]
    fn a_missing_epoch_reads_as_zero() {
        assert_eq!(cmp("1.0", "0:1.0"), Equal);
    }

    #[test]
    fn tilde_sorts_before_everything_including_the_end_of_the_string() {
        // This is what makes "1.0~rc1" a pre-release of "1.0" rather than
        // a later version of it.
        assert_eq!(cmp("1.0~rc1", "1.0"), Less);
        assert_eq!(cmp("1.0~~", "1.0~"), Less);
        assert_eq!(cmp("1.0~", "1.0"), Less);
        assert_eq!(cmp("1.0~rc1", "1.0~rc2"), Less);
    }

    #[test]
    fn letters_sort_before_other_non_digit_characters() {
        assert_eq!(cmp("1.0a", "1.0+"), Less);
    }

    #[test]
    fn the_debian_revision_breaks_a_tie_on_upstream() {
        assert_eq!(cmp("1.0-1", "1.0-2"), Less);
        assert_eq!(cmp("1.0-1", "1.0"), Greater);
    }

    #[test]
    fn orders_the_openssl_versions_from_the_osv_fixture() {
        // fixtures/osv_debian_openssl.json: DLA-3942-1 is fixed in
        // 1.1.1n-0+deb11u6, and lists u1..u5 as affected.
        assert_eq!(cmp("1.1.1n-0+deb11u3", "1.1.1n-0+deb11u6"), Less);
        assert_eq!(cmp("1.1.1n-0+deb11u6", "1.1.1n-0+deb11u5"), Greater);
        assert_eq!(cmp("1.1.1k-1", "1.1.1n-0+deb11u1"), Less);
        assert_eq!(cmp("1.1.1l-1", "1.1.1m-1"), Less);
    }

    #[test]
    fn comparison_is_antisymmetric() {
        for (a, b) in [
            ("1.0", "1.1"),
            // An epoch wins outright, so the lesser side is the one without.
            ("2.0", "1:1.0"),
            ("1.0~rc1", "1.0"),
            ("1.1.1n-0+deb11u3", "1.1.1n-0+deb11u6"),
        ] {
            assert_eq!(cmp(a, b), Less, "{a} < {b}");
            assert_eq!(cmp(b, a), Greater, "{b} > {a}");
        }
    }
}
