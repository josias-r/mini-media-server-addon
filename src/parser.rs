use regex::Regex;

pub fn is_definitely_wrong_episode(
    title: &str,
    expected_season: u32,
    expected_episode: u32,
) -> bool {
    let str = title.to_uppercase();

    // Pattern 1: S01E05 or S01E05-E06 or S01E05-06
    let sxx_exx_match = Regex::new(r"S(\d+)E(\d+)(?:(?:-E|-)(\d+))?").unwrap();
    if let Some(caps) = sxx_exx_match.captures(&str) {
        let s: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
        let e_start: u32 = caps.get(2).unwrap().as_str().parse().unwrap();
        let e_end: u32 = caps.get(3).map_or(e_start, |m| m.as_str().parse().unwrap());

        if s == expected_season {
            if expected_episode >= e_start && expected_episode <= e_end {
                return false; // It's our episode
            }
            return true; // Right season, WRONG episode
        } else {
            return true; // Wrong season entirely
        }
    }

    // Pattern 2: 1x05 or 1x05-06
    let ax_b_match = Regex::new(r"(?:^|[\s_\[-])(\d+)X(\d+)(?:-(\d+))?(?:[\s_\]-]|$)").unwrap();
    if let Some(caps) = ax_b_match.captures(&str) {
        let s: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
        let e_start: u32 = caps.get(2).unwrap().as_str().parse().unwrap();
        let e_end: u32 = caps.get(3).map_or(e_start, |m| m.as_str().parse().unwrap());

        if s == expected_season {
            if expected_episode >= e_start && expected_episode <= e_end {
                return false;
            }
            return true;
        } else {
            if s < 30 {
                return true;
            }
        }
    }

    // Pattern 3: Absolute episode numbers like E05 or EP05 or Episode 5
    let ep_match =
        Regex::new(r"(?:^|[^A-Z])(?:E|EP|EPISODE)[.\s-]*(\d+)(?:(?:-E?|-)(\d+))?(?:\b|$)").unwrap();
    if let Some(caps) = ep_match.captures(&str) {
        let e_start: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
        let e_end: u32 = caps.get(2).map_or(e_start, |m| m.as_str().parse().unwrap());

        if expected_episode >= e_start && expected_episode <= e_end {
            return false;
        }
        return true;
    }

    false
}

pub fn is_definitely_not_full_season(title: &str, expected_season: u32) -> bool {
    let str = title.to_uppercase();

    let sxx_exx_match = Regex::new(r"S(\d+)E(\d+)(?:(?:-E|-)(\d+))?").unwrap();
    if let Some(caps) = sxx_exx_match.captures(&str) {
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

    let ax_b_match = Regex::new(r"(?:^|[\s_\[-])(\d+)X(\d+)(?:-(\d+))?(?:[\s_\]-]|$)").unwrap();
    if let Some(caps) = ax_b_match.captures(&str) {
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

    let ep_match =
        Regex::new(r"(?:^|[^A-Z])(?:E|EP|EPISODE)[.\s-]*(\d+)(?:(?:-E?|-)(\d+))?(?:\b|$)").unwrap();
    if let Some(caps) = ep_match.captures(&str) {
        let e_start: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
        let e_end: u32 = caps.get(2).map_or(e_start, |m| m.as_str().parse().unwrap());
        if e_start == e_end {
            return true;
        }
    }

    let season_match =
        Regex::new(r"(?:^|[^A-Z])(?:S|SEASON)[.\s-]*(\d+)(?:(?:-S?|-)(\d+))?(?:\b|$)").unwrap();
    if let Some(caps) = season_match.captures(&str) {
        let s_start: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
        let s_end: u32 = caps.get(2).map_or(s_start, |m| m.as_str().parse().unwrap());

        if expected_season < s_start || expected_season > s_end {
            return true;
        }
    }

    false
}
