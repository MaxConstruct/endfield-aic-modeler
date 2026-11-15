//! How numbers look on screen. Values stay at full precision everywhere else; this only rounds for
//! display. An undetermined value (`None`) is blank, never 0 or ∞.

/// Up to two decimals, rounded half away from zero, trailing zeros dropped: 1.125 shows as 1.13,
/// 1/3 as 0.33, 0.5 as 0.5, 2 as 2.
pub fn short(value: Option<f64>) -> String {
    let Some(v) = value else {
        return String::new();
    };
    let rounded = round2(v);
    // Every rate on the canvas goes through here every frame: whole hundredths are written as
    // integers, which is much cheaper than formatting a float. Past 1e13 a hundredth is near the
    // float's precision (and NaN or infinity isn't a number of hundredths), so `{:.2}` decides.
    if rounded.is_nan() || rounded.abs() >= 1e13 {
        let text = format!("{rounded:.2}");
        return text.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    let hundredths = (rounded * 100.0).round() as i64;
    let sign = if hundredths < 0 { "-" } else { "" };
    let (whole, fraction) = (hundredths.abs() / 100, hundredths.abs() % 100);
    match fraction {
        0 => format!("{sign}{whole}"),
        tenths if tenths % 10 == 0 => format!("{sign}{whole}.{}", tenths / 10),
        _ => format!("{sign}{whole}.{fraction:02}"),
    }
}

/// A value as it shows in a field the player edits: up to four decimals, trailing zeros dropped.
pub fn editable(value: f64) -> String {
    let text = format!("{:.4}", (value * 1e4).round() / 1e4);
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Rounds to two decimals, half away from zero. `{:.2}` alone rounds 1.125 down to 1.12.
fn round2(v: f64) -> f64 {
    // The solver's 1.125 can come back as 1.12499999…; settle such noise before rounding.
    let settled = (v * 1e6).round() / 1e6;
    let rounded = (settled * 100.0).round() / 100.0;
    // Avoid showing "-0".
    if rounded == 0.0 { 0.0 } else { rounded }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_rounds_half_away_from_zero() {
        assert_eq!(short(Some(1.125)), "1.13");
        assert_eq!(short(Some(1.0 / 3.0)), "0.33");
        assert_eq!(short(Some(0.375)), "0.38");
        assert_eq!(short(Some(-1e-12)), "0");
        assert_eq!(short(Some(1.125 - 1e-10)), "1.13", "solver noise");
    }

    #[test]
    fn short_drops_trailing_zeros() {
        assert_eq!(short(Some(0.4)), "0.4");
        assert_eq!(short(Some(0.5)), "0.5");
        assert_eq!(short(Some(1.25)), "1.25");
        assert_eq!(short(Some(1.0)), "1");
        assert_eq!(short(Some(120.0)), "120");
    }

    #[test]
    fn editable_keeps_four_decimals() {
        assert_eq!(editable(5.0), "5");
        assert_eq!(editable(225.0), "225");
        assert_eq!(editable(1.0 / 3.0), "0.3333");
    }

    #[test]
    fn short_writes_what_two_decimals_of_the_float_would() {
        let by_float = |v: f64| {
            let text = format!("{:.2}", round2(v));
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        };
        let mut values = vec![
            0.0, -0.0, 0.004, -0.004, 0.005, -0.005, 0.995, 9.995, -1.125, 1e12,
        ];
        values.extend([
            1e13,
            -1e13,
            3e15,
            f64::MAX,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ]);
        // A spread of rates, their thirds and sevenths, signs and sizes.
        for i in -20_000..20_000 {
            let v = f64::from(i) * 0.0137;
            values.extend([v, v / 3.0, v / 7.0, v * 1e5 / 7.0]);
        }
        for v in values {
            assert_eq!(short(Some(v)), by_float(v), "{v}");
        }
    }

    #[test]
    fn undetermined_is_blank() {
        assert_eq!(short(None), "");
    }
}
