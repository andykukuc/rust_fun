//! RPM version ordering, per `rpmvercmp` in rpm's `rpmvercmp.c`.

use core::cmp::Ordering;

/// Compare two RPM EVR strings: `[epoch:]version[-release]`.
pub fn compare(left: &str, right: &str) -> Ordering {
    let (left_epoch, left_version, left_release) = split_evr(left);
    let (right_epoch, right_version, right_release) = split_evr(right);

    left_epoch
        .cmp(&right_epoch)
        .then_with(|| rpmvercmp(left_version, right_version))
        .then_with(|| rpmvercmp(left_release, right_release))
}

/// Split `[epoch:]version[-release]`. An absent epoch reads as 0.
fn split_evr(value: &str) -> (u64, &str, &str) {
    let (epoch, rest) = match value.find(':') {
        Some(colon) if colon > 0 && value[..colon].bytes().all(|b| b.is_ascii_digit()) => {
            (value[..colon].parse().unwrap_or(0), &value[colon + 1..])
        }
        _ => (0, value),
    };
    match rest.rfind('-') {
        Some(hyphen) => (epoch, &rest[..hyphen], &rest[hyphen + 1..]),
        None => (epoch, rest, ""),
    }
}

fn strip_leading_zeros(segment: &[u8]) -> &[u8] {
    let first = segment
        .iter()
        .position(|b| *b != b'0')
        .unwrap_or(segment.len());
    &segment[first..]
}

fn is_separator(byte: u8) -> bool {
    !byte.is_ascii_alphanumeric() && byte != b'~' && byte != b'^'
}

/// rpm's `rpmvercmp`: walk alphanumeric segments, treating everything else as
/// a separator, except `~` (sorts below everything) and `^` (sorts above the
/// bare version).
fn rpmvercmp(left: &str, right: &str) -> Ordering {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    let (mut i, mut j) = (0usize, 0usize);

    loop {
        while i < left.len() && is_separator(left[i]) {
            i += 1;
        }
        while j < right.len() && is_separator(right[j]) {
            j += 1;
        }

        let left_tilde = i < left.len() && left[i] == b'~';
        let right_tilde = j < right.len() && right[j] == b'~';
        if left_tilde || right_tilde {
            if !left_tilde {
                return Ordering::Greater;
            }
            if !right_tilde {
                return Ordering::Less;
            }
            i += 1;
            j += 1;
            continue;
        }

        let left_caret = i < left.len() && left[i] == b'^';
        let right_caret = j < right.len() && right[j] == b'^';
        if left_caret || right_caret {
            if i >= left.len() {
                return Ordering::Less;
            }
            if j >= right.len() {
                return Ordering::Greater;
            }
            if !left_caret {
                return Ordering::Greater;
            }
            if !right_caret {
                return Ordering::Less;
            }
            i += 1;
            j += 1;
            continue;
        }

        if i >= left.len() || j >= right.len() {
            break;
        }

        let numeric = left[i].is_ascii_digit();
        let (start_left, start_right) = (i, j);
        if numeric {
            while i < left.len() && left[i].is_ascii_digit() {
                i += 1;
            }
            while j < right.len() && right[j].is_ascii_digit() {
                j += 1;
            }
        } else {
            while i < left.len() && left[i].is_ascii_alphabetic() {
                i += 1;
            }
            while j < right.len() && right[j].is_ascii_alphabetic() {
                j += 1;
            }
        }

        let left_segment = &left[start_left..i];
        let right_segment = &right[start_right..j];

        // An empty segment on the right means the two sides disagree on
        // segment kind: a numeric segment outranks an alphabetic one.
        if right_segment.is_empty() {
            return if numeric {
                Ordering::Greater
            } else {
                Ordering::Less
            };
        }

        let ordering = if numeric {
            let left_digits = strip_leading_zeros(left_segment);
            let right_digits = strip_leading_zeros(right_segment);
            left_digits
                .len()
                .cmp(&right_digits.len())
                .then_with(|| left_digits.cmp(right_digits))
        } else {
            left_segment.cmp(right_segment)
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }

    match (i >= left.len(), j >= right.len()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => Ordering::Equal,
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
        assert_eq!(cmp("3.0.7-6.el9", "3.0.7-6.el9"), Equal);
    }

    #[test]
    fn numeric_segments_compare_as_numbers() {
        assert_eq!(cmp("1.10", "1.9"), Greater);
        assert_eq!(cmp("1.01", "1.1"), Equal);
    }

    #[test]
    fn a_numeric_segment_outranks_an_alphabetic_one() {
        assert_eq!(cmp("1.1", "1.a"), Greater);
    }

    #[test]
    fn non_alphanumeric_characters_are_only_separators() {
        assert_eq!(cmp("1.0.1", "1_0_1"), Equal);
    }

    #[test]
    fn tilde_sorts_before_everything() {
        assert_eq!(cmp("1.0~rc1", "1.0"), Less);
        assert_eq!(cmp("1.0~rc1", "1.0~rc2"), Less);
    }

    #[test]
    fn caret_sorts_after_the_bare_version() {
        assert_eq!(cmp("1.0", "1.0^"), Less);
        assert_eq!(cmp("1.0^", "1.0"), Greater);
    }

    #[test]
    fn an_epoch_outranks_the_version() {
        assert_eq!(cmp("1:1.0", "2.0"), Greater);
        assert_eq!(cmp("1.0", "0:1.0"), Equal);
    }

    #[test]
    fn the_release_breaks_a_tie_on_version() {
        assert_eq!(cmp("3.0.7-5.el9", "3.0.7-6.el9"), Less);
    }

    #[test]
    fn a_two_digit_rhel_release_outranks_a_one_digit_one() {
        // The el9 / el10 trap: text ordering would get this backwards.
        assert_eq!(cmp("1.0-1.el10", "1.0-1.el9"), Greater);
    }
}
