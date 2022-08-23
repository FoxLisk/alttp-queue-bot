
// lifted from league bot
// i really need a way to stop copy-pasting code across projects
// this is extended to handle fractions though

/// returns 1:23:45 for integral seconds,
/// 1:23:45.67 for fractional seconds
pub fn format_hms(secs: f64) -> String {
    let mins = secs as u64 / 60;
    let hours = mins / 60;
    let secs_residue = secs - ((secs as u64 - (secs as u64 % 60)) as f64);
    let secs_fmt = if secs_residue.fract() == 0.0 {
        format!("{:02}", secs_residue as u64)
    } else {
        format!("{:02.2}", secs_residue)
    };
    if hours > 0 {
        format!(
            "{hours}h{mins:02}m{secs}s",
            hours = hours,
            mins = mins % 60,
            secs=secs_fmt
        )
    } else {
        format!(
            "{mins}m{secs}s",
            mins = mins % 60,
            secs=secs_fmt
        )
    }
}

pub fn secs_to_millis(secs: f64) -> u64 {
    let millis = secs * 1000.0;
    millis.ceil() as u64
}

/// Gets env var, panics if it's missing
pub fn env_var(key: &str) -> String {
    std::env::var(key).expect(&format!("Missing environment variable: `{}`", key))
}


mod tests {
    use crate::utils::{format_hms,};

    #[test]
    fn test_format() {
        let secs = 45 + (60 * 23) + (60 * 60);
        assert_eq!("1:23:45", format_hms(secs as f64));
        assert_eq!("1:23:45.67", format_hms(secs as f64 + 0.67));
    }
}