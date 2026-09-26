#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct ReleaseCadence {
    pub median_days:    f64,
    pub last_days_ago:  i64,
    pub tag_count:      usize,
}

#[allow(dead_code)]
pub fn release_cadence(path: &std::path::Path) -> Option<ReleaseCadence> {
    let out = std::process::Command::new("git")
        .arg("-C").arg(path)
        .args(["for-each-ref", "refs/tags", "--format=%(creatordate:unix)", "--sort=creatordate"])
        .output().ok()?;
    let mut ts: Vec<i64> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().parse::<i64>().ok())
        .collect();
    ts.sort_unstable();
    if ts.len() < 3 { return None; }

    let mut gaps: Vec<i64> = ts.windows(2).map(|w| (w[1] - w[0]) / 86400).collect();
    gaps.sort_unstable();
    let mid = gaps.len() / 2;
    let median_days = if gaps.len() % 2 == 1 {
        gaps[mid] as f64
    } else {
        (gaps[mid - 1] + gaps[mid]) as f64 / 2.0
    };

    let last = *ts.last()?;
    let last_days_ago = (chrono::Utc::now().timestamp() - last) / 86400;
    Some(ReleaseCadence { median_days, last_days_ago, tag_count: ts.len() })
}
