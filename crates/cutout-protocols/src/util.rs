pub(crate) fn u64_to_i64_saturating(value: u64) -> i64 {
    i64::try_from(value).map_or(i64::MAX, |value| value)
}
