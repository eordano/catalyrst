use crate::TimeRange;

pub const MS_PER_DAY: i64 = 86_400_000;
pub const MS_PER_WEEK: i64 = 7 * MS_PER_DAY;
pub const MS_PER_MONTH: i64 = 4 * MS_PER_WEEK;
pub const MS_PER_YEAR: i64 = 12 * MS_PER_MONTH;

#[derive(Debug, Clone)]
pub struct TimeRangeDivision {
    pub intervals: Vec<TimeRange>,
    pub remainder: TimeRange,
}

pub fn interval_size_label(time_range: &TimeRange) -> &'static str {
    let diff = time_range_size_ms(time_range);
    match diff {
        d if d == MS_PER_DAY => "day",
        d if d == MS_PER_WEEK => "week",
        d if d == MS_PER_MONTH => "month",
        d if d == MS_PER_YEAR => "year",
        _ => "unknown",
    }
}

pub fn time_range_size_ms(time_range: &TimeRange) -> i64 {
    time_range.end_timestamp - time_range.init_timestamp
}

pub fn is_time_range_covered_by(target: &TimeRange, ranges: &[TimeRange]) -> bool {
    if ranges.is_empty() {
        return false;
    }
    let min_timestamp = ranges[0].init_timestamp;
    let mut current_max = ranges[0].end_timestamp;
    for t in ranges.iter().skip(1) {
        if t.init_timestamp > current_max {
            return false;
        }
        current_max = current_max.max(t.end_timestamp);
    }
    min_timestamp <= target.init_timestamp && current_max >= target.end_timestamp
}

pub fn divide_time_in_years_months_weeks_and_days(time_range: &TimeRange) -> TimeRangeDivision {
    let interval_sizes = [MS_PER_YEAR, MS_PER_MONTH, MS_PER_WEEK, MS_PER_DAY];
    let total_size = time_range_size_ms(time_range);
    let mut intervals = Vec::new();
    let mut remaining = total_size;
    let mut init = time_range.init_timestamp;

    for (idx, &interval_size) in interval_sizes.iter().enumerate() {
        let next_size = interval_sizes.get(idx + 1).copied().unwrap_or(interval_size);
        let num_next_in_current = interval_size / next_size;

        loop {
            let min_needed = num_next_in_current * next_size
                + interval_sizes.get(idx + 1).copied().unwrap_or(0)
                + interval_sizes.get(idx + 2).copied().unwrap_or(0)
                + interval_sizes.get(idx + 3).copied().unwrap_or(0);

            if remaining < min_needed {
                break;
            }

            let end = init + interval_size;
            intervals.push(TimeRange {
                init_timestamp: init,
                end_timestamp: end,
            });
            init = end;
            remaining -= interval_size;
        }
    }

    TimeRangeDivision {
        intervals,
        remainder: TimeRange {
            init_timestamp: init,
            end_timestamp: time_range.end_timestamp,
        },
    }
}

pub fn join_overlapped_time_ranges(ranges: &[TimeRange]) -> Vec<TimeRange> {
    if ranges.is_empty() {
        return Vec::new();
    }

    let mut sorted: Vec<TimeRange> = ranges.to_vec();
    sorted.sort_by_key(|r| r.init_timestamp);

    let mut result = Vec::new();
    let mut init = sorted[0].init_timestamp;
    let mut end = sorted[0].end_timestamp;

    for r in sorted.iter().skip(1) {
        if r.init_timestamp > end {
            result.push(TimeRange {
                init_timestamp: init,
                end_timestamp: end,
            });
            init = r.init_timestamp;
            end = r.end_timestamp;
        } else {
            end = end.max(r.end_timestamp);
        }
    }
    result.push(TimeRange {
        init_timestamp: init,
        end_timestamp: end,
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_time_range_covered_empty() {
        let target = TimeRange {
            init_timestamp: 0,
            end_timestamp: 100,
        };
        assert!(!is_time_range_covered_by(&target, &[]));
    }

    #[test]
    fn test_is_time_range_covered_exact() {
        let target = TimeRange {
            init_timestamp: 0,
            end_timestamp: 100,
        };
        let ranges = vec![TimeRange {
            init_timestamp: 0,
            end_timestamp: 100,
        }];
        assert!(is_time_range_covered_by(&target, &ranges));
    }

    #[test]
    fn test_is_time_range_covered_gap() {
        let target = TimeRange {
            init_timestamp: 0,
            end_timestamp: 100,
        };
        let ranges = vec![
            TimeRange {
                init_timestamp: 0,
                end_timestamp: 40,
            },
            TimeRange {
                init_timestamp: 60,
                end_timestamp: 100,
            },
        ];
        assert!(!is_time_range_covered_by(&target, &ranges));
    }

    #[test]
    fn test_is_time_range_covered_overlap() {
        let target = TimeRange {
            init_timestamp: 0,
            end_timestamp: 100,
        };
        let ranges = vec![
            TimeRange {
                init_timestamp: 0,
                end_timestamp: 60,
            },
            TimeRange {
                init_timestamp: 50,
                end_timestamp: 100,
            },
        ];
        assert!(is_time_range_covered_by(&target, &ranges));
    }

    #[test]
    fn test_is_time_range_covered_superset() {
        let target = TimeRange {
            init_timestamp: 10,
            end_timestamp: 90,
        };
        let ranges = vec![
            TimeRange {
                init_timestamp: 0,
                end_timestamp: 50,
            },
            TimeRange {
                init_timestamp: 50,
                end_timestamp: 100,
            },
        ];
        assert!(is_time_range_covered_by(&target, &ranges));
    }

    #[test]
    fn test_divide_simple() {
        let total = MS_PER_YEAR + MS_PER_MONTH + MS_PER_WEEK + MS_PER_DAY;
        let tr = TimeRange {
            init_timestamp: 0,
            end_timestamp: total,
        };
        let div = divide_time_in_years_months_weeks_and_days(&tr);
        assert_eq!(div.intervals.len(), 4);
        assert_eq!(time_range_size_ms(&div.intervals[0]), MS_PER_YEAR);
        assert_eq!(time_range_size_ms(&div.intervals[1]), MS_PER_MONTH);
        assert_eq!(time_range_size_ms(&div.intervals[2]), MS_PER_WEEK);
        assert_eq!(time_range_size_ms(&div.intervals[3]), MS_PER_DAY);
    }

    #[test]
    fn test_join_overlapped_empty() {
        assert!(join_overlapped_time_ranges(&[]).is_empty());
    }

    #[test]
    fn test_join_overlapped_no_overlap() {
        let ranges = vec![
            TimeRange {
                init_timestamp: 0,
                end_timestamp: 10,
            },
            TimeRange {
                init_timestamp: 20,
                end_timestamp: 30,
            },
        ];
        let result = join_overlapped_time_ranges(&ranges);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_join_overlapped_merge() {
        let ranges = vec![
            TimeRange {
                init_timestamp: 0,
                end_timestamp: 15,
            },
            TimeRange {
                init_timestamp: 10,
                end_timestamp: 30,
            },
        ];
        let result = join_overlapped_time_ranges(&ranges);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].init_timestamp, 0);
        assert_eq!(result[0].end_timestamp, 30);
    }

    #[test]
    fn test_join_overlapped_adjacent() {
        let ranges = vec![
            TimeRange {
                init_timestamp: 0,
                end_timestamp: 10,
            },
            TimeRange {
                init_timestamp: 10,
                end_timestamp: 20,
            },
        ];
        let result = join_overlapped_time_ranges(&ranges);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].init_timestamp, 0);
        assert_eq!(result[0].end_timestamp, 20);
    }

    #[test]
    fn test_join_overlapped_subset() {
        let ranges = vec![
            TimeRange {
                init_timestamp: 0,
                end_timestamp: 30,
            },
            TimeRange {
                init_timestamp: 5,
                end_timestamp: 20,
            },
        ];
        let result = join_overlapped_time_ranges(&ranges);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].init_timestamp, 0);
        assert_eq!(result[0].end_timestamp, 30);
    }

    #[test]
    fn test_interval_size_label_day() {
        let tr = TimeRange {
            init_timestamp: 0,
            end_timestamp: MS_PER_DAY,
        };
        assert_eq!(interval_size_label(&tr), "day");
    }

    #[test]
    fn test_interval_size_label_unknown() {
        let tr = TimeRange {
            init_timestamp: 0,
            end_timestamp: 12345,
        };
        assert_eq!(interval_size_label(&tr), "unknown");
    }

    fn division_repr(time_range: &TimeRange) -> String {
        let div = divide_time_in_years_months_weeks_and_days(time_range);
        let mut repr = String::new();
        for interval in &div.intervals {
            match time_range_size_ms(interval) {
                d if d == MS_PER_DAY => repr.push('I'),
                d if d == MS_PER_WEEK => repr.push('W'),
                d if d == MS_PER_MONTH => repr.push('M'),
                d if d == MS_PER_YEAR => repr.push('Y'),
                _ => repr.push('?'),
            }
        }

        if !div.intervals.is_empty() {
            assert_eq!(
                div.intervals[0].init_timestamp, time_range.init_timestamp,
                "First interval must start at time_range start"
            );
            assert_eq!(
                div.remainder.end_timestamp, time_range.end_timestamp,
                "Remainder must end at time_range end"
            );
            let mut prev_end = div.intervals[0].init_timestamp;
            for interval in &div.intervals {
                assert_eq!(
                    interval.init_timestamp, prev_end,
                    "Intervals must be contiguous"
                );
                prev_end = interval.end_timestamp;
            }
            assert_eq!(
                div.remainder.init_timestamp,
                div.intervals.last().unwrap().end_timestamp,
                "Remainder must start where last interval ends"
            );
        }

        repr
    }

    fn time_range_of_days(start: i64, num_days: i64) -> TimeRange {
        TimeRange {
            init_timestamp: start,
            end_timestamp: start + MS_PER_DAY * num_days,
        }
    }

    #[test]
    fn test_progression_key_boundaries() {
        let start = 1_640_995_200_000i64;

        assert_eq!(division_repr(&time_range_of_days(start, 1)), "I");
        assert_eq!(division_repr(&time_range_of_days(start, 7)), "IIIIIII");
        assert_eq!(division_repr(&time_range_of_days(start, 8)), "WI");
        assert_eq!(division_repr(&time_range_of_days(start, 14)), "WIIIIIII");
        assert_eq!(division_repr(&time_range_of_days(start, 15)), "WWI");
        assert_eq!(division_repr(&time_range_of_days(start, 21)), "WWIIIIIII");
        assert_eq!(division_repr(&time_range_of_days(start, 22)), "WWWI");
        assert_eq!(division_repr(&time_range_of_days(start, 28)), "WWWIIIIIII");
        assert_eq!(division_repr(&time_range_of_days(start, 29)), "WWWWI");
        assert_eq!(division_repr(&time_range_of_days(start, 34)), "WWWWIIIIII");
        assert_eq!(division_repr(&time_range_of_days(start, 35)), "WWWWIIIIIII");
        assert_eq!(division_repr(&time_range_of_days(start, 36)), "MWI");
        assert_eq!(division_repr(&time_range_of_days(start, 42)), "MWIIIIIII");
        assert_eq!(division_repr(&time_range_of_days(start, 43)), "MWWI");
        assert_eq!(division_repr(&time_range_of_days(start, 64)), "MMWI");
        assert_eq!(
            division_repr(&time_range_of_days(start, 364)),
            "MMMMMMMMMMMMWWWIIIIIII"
        );
        assert_eq!(
            division_repr(&time_range_of_days(start, 365)),
            "MMMMMMMMMMMMWWWWI"
        );
        assert_eq!(
            division_repr(&time_range_of_days(start, 371)),
            "MMMMMMMMMMMMWWWWIIIIIII"
        );
        assert_eq!(division_repr(&time_range_of_days(start, 372)), "YMWI");
    }

    #[test]
    fn test_divide_two_days_plus_remainder() {
        let start = 1_640_995_200_000i64;
        let tr = TimeRange {
            init_timestamp: start,
            end_timestamp: start + 2 * MS_PER_DAY + 50_000,
        };
        let div = divide_time_in_years_months_weeks_and_days(&tr);
        assert_eq!(div.intervals.len(), 2);
        for interval in &div.intervals {
            assert_eq!(time_range_size_ms(interval), MS_PER_DAY);
        }
        assert_eq!(div.intervals[0].init_timestamp, start);
        assert_eq!(div.intervals[0].end_timestamp, start + MS_PER_DAY);
        assert_eq!(div.intervals[1].init_timestamp, start + MS_PER_DAY);
        assert_eq!(div.intervals[1].end_timestamp, start + 2 * MS_PER_DAY);
        assert_eq!(time_range_size_ms(&div.remainder), 50_000);
    }
}
