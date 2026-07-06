//! Release cadence: how often a repo actually ships, from local tags.

/// Per-repo release cadence, derived from local tag `creatordate`s (a tag
/// made through the GitHub web UI only exists locally after a fetch — that's
/// fine, since this is meant to reflect what you can see without a network
/// round-trip; reconciling with the provider's releases API happens
/// separately during `[R]` refresh).
// kept: Sprint D Stats-tab redesign — no persist needed (Tier A, cheap enough
// to recompute per session), so this is a pure on-demand query for now.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct ReleaseCadence {
    /// Median days between consecutive tags — median, not mean, so one long
    /// dry spell can't dominate the number.
    pub median_days:    f64,
    pub last_days_ago:  i64,
    pub tag_count:      usize,
}

/// Computes cadence for `path`, or `None` when there are fewer than 3 tags
/// (a median from 1-2 samples doesn't mean anything).
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
