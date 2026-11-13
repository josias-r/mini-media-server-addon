use regex::Regex;
use std::sync::LazyLock;

static SXX_EXX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"S(\d+)E(\d+)(?:(?:-E|-)(\d+))?").unwrap());
static AX_B_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|[\s_\[-])(\d+)X(\d+)(?:-(\d+))?(?:[\s_\]-]|$)").unwrap());
static EP_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[^A-Z])(?:E|EP|EPISODE)[.\s-]*(\d+)(?:(?:-E?|-)(\d+))?(?:\b|$)").unwrap()
});
static SEASON_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[^A-Z])(?:S|SEASON)[.\s-]*(\d+)(?:(?:-S?|-)(\d+))?(?:\b|$)").unwrap()
});

const VIDEO_EXTENSIONS: [&str; 5] = ["mp4", "mkv", "avi", "webm", "mov"];

pub fn is_video_file(name: &str) -> bool {
    let Some(ext) = name.rsplit('.').next() else {
        return false;
    };
    if name == ext {
        return false;
    }
    let ext_lower = ext.to_lowercase();
    VIDEO_EXTENSIONS.iter().any(|&v| v == ext_lower)
}

pub fn is_definitely_wrong_episode(
    title: &str,
    expected_season: u32,
    expected_episode: u32,
) -> bool {
    let upper = title.to_uppercase();

    // Pattern 1: S01E05 or S01E05-E06 or S01E05-06
    if let Some(caps) = SXX_EXX_RE.captures(&upper) {
        let s: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
        let e_start: u32 = caps.get(2).unwrap().as_str().parse().unwrap();
        let e_end: u32 = caps.get(3).map_or(e_start, |m| m.as_str().parse().unwrap());

        if s == expected_season {
            return !(expected_episode >= e_start && expected_episode <= e_end);
        }
        return true;
    }

    // Pattern 2: 1x05 or 1x05-06
    if let Some(caps) = AX_B_RE.captures(&upper) {
        let s: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
        let e_start: u32 = caps.get(2).unwrap().as_str().parse().unwrap();
        let e_end: u32 = caps.get(3).map_or(e_start, |m| m.as_str().parse().unwrap());

        if s == expected_season {
            return !(expected_episode >= e_start && expected_episode <= e_end);
        } else if s < 30 {
            return true;
        }
    }

    // Pattern 3: Absolute episode numbers like E05 or EP05 or Episode 5
    if let Some(caps) = EP_RE.captures(&upper) {
        let e_start: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
        let e_end: u32 = caps.get(2).map_or(e_start, |m| m.as_str().parse().unwrap());
        return !(expected_episode >= e_start && expected_episode <= e_end);
    }

    false
}

pub fn is_definitely_not_full_season(title: &str, expected_season: u32) -> bool {
    let upper = title.to_uppercase();

    if let Some(caps) = SXX_EXX_RE.captures(&upper) {
        let s: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
        if s != expected_season {
            return true;
        }
        let e_start: u32 = caps.get(2).unwrap().as_str().parse().unwrap();
        let e_end: u32 = caps.get(3).map_or(e_start, |m| m.as_str().parse().unwrap());
        if e_start == e_end {
            return true;
        }
    }

    if let Some(caps) = AX_B_RE.captures(&upper) {
        let s: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
        if s < 30 {
            if s != expected_season {
                return true;
            }
            let e_start: u32 = caps.get(2).unwrap().as_str().parse().unwrap();
            let e_end: u32 = caps.get(3).map_or(e_start, |m| m.as_str().parse().unwrap());
            if e_start == e_end {
                return true;
            }
        }
    }

    if let Some(caps) = EP_RE.captures(&upper) {
        let e_start: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
        let e_end: u32 = caps.get(2).map_or(e_start, |m| m.as_str().parse().unwrap());
        if e_start == e_end {
            return true;
        }
    }

    if let Some(caps) = SEASON_RE.captures(&upper) {
        let s_start: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
        let s_end: u32 = caps.get(2).map_or(s_start, |m| m.as_str().parse().unwrap());

        if expected_season < s_start || expected_season > s_end {
            return true;
        }
    }

    false
}
