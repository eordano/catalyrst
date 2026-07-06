#[derive(Debug, Clone)]
pub enum ParsedRange {
    Range { start: u64, end: u64 },
    Unsatisfiable,
}

pub fn parse_range_header(
    range_header: Option<&str>,
    total_size: Option<u64>,
) -> Option<ParsedRange> {
    let header = range_header?;
    let total = total_size?;

    if let Some(rest) = header.strip_prefix("bytes=") {
        if let Some(suffix) = rest.strip_prefix('-') {
            let suffix_len: u64 = suffix.parse().ok()?;
            if suffix_len == 0 || total == 0 {
                return Some(ParsedRange::Unsatisfiable);
            }
            let start = total.saturating_sub(suffix_len);
            return Some(ParsedRange::Range {
                start,
                end: total - 1,
            });
        }

        let parts: Vec<&str> = rest.splitn(2, '-').collect();
        if parts.len() == 2 {
            let start: u64 = parts[0].parse().ok()?;
            let end: u64 = if parts[1].is_empty() {
                total - 1
            } else {
                parts[1].parse().ok()?
            };

            if start > end || start >= total {
                return Some(ParsedRange::Unsatisfiable);
            }

            return Some(ParsedRange::Range {
                start,
                end: end.min(total - 1),
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_range_normal() {
        let r = parse_range_header(Some("bytes=0-499"), Some(1000));
        match r {
            Some(ParsedRange::Range { start, end }) => {
                assert_eq!(start, 0);
                assert_eq!(end, 499);
            }
            _ => panic!("Expected Range"),
        }
    }

    #[test]
    fn parse_range_suffix() {
        let r = parse_range_header(Some("bytes=-200"), Some(1000));
        match r {
            Some(ParsedRange::Range { start, end }) => {
                assert_eq!(start, 800);
                assert_eq!(end, 999);
            }
            _ => panic!("Expected Range"),
        }
    }

    #[test]
    fn parse_range_unsatisfiable() {
        let r = parse_range_header(Some("bytes=1000-2000"), Some(500));
        assert!(matches!(r, Some(ParsedRange::Unsatisfiable)));
    }
}
