use regex::Regex;
use std::sync::LazyLock;

static SXX_EXX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[^A-Z0-9])S(\d+)[.\s_-]*E(\d+)(?:(?:-?E|-)(\d+))?(?:V\d+)?(?:[^A-Z0-9]|$)")
        .unwrap()
});
static AX_B_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[\s_.\[\(\-])(\d+)X(\d+)(?:-(\d+))?(?:[\s_.\]\)\-]|$)").unwrap()
});
static EP_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:^|[^A-Z0-9])(?:E|EP|EPISODE)[.\s_-]*(\d+)(?:(?:-?E|-)(\d+))?(?:V\d+)?(?:[^A-Z0-9]|$)",
    )
    .unwrap()
});
static SEASON_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:^|[^A-Z0-9])(?:S|SEASON)[.\s_-]*(\d+)(?:(?:[\s._-]*(?:TO[\s._-]*)?S|-)(\d+))?(?:[^A-Z0-9]|$)",
    )
    .unwrap()
});
static BARE_LEADING_EP_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d{1,3})(?:-(\d{1,3}))?(?:V\d+)?(?:[.\s_-]+[^0-9]|[.\s_-]*$)").unwrap()
});
static BARE_DASH_EP_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|\s)-\s+(\d{1,3})(?:-(\d{1,3}))?(?:V\d+)?(?:\s|\[|\(|$|_|\.)").unwrap()
});
static BARE_BRACKET_EP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[(\d{1,3})(?:-(\d{1,3}))?(?:V\d+)?\]").unwrap());

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

fn strip_video_extension(name: &str) -> &str {
    if is_video_file(name)
        && let Some((stem, _)) = name.rsplit_once('.')
    {
        return stem;
    }
    name
}

fn is_likely_non_episode_number(num: u32) -> bool {
    matches!(num, 264 | 265 | 480 | 576 | 720)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodeInfo {
    pub season_ranges: Option<Vec<(u32, u32)>>,
    pub ep_start: u32,
    pub ep_end: u32,
    pub from_basename: bool,
}

struct ParsedEpisode {
    season: Option<(u32, u32)>,
    ep_start: u32,
    ep_end: u32,
}

fn extract_episode_from_segment(segment: &str) -> Option<ParsedEpisode> {
    let upper = strip_video_extension(segment).to_uppercase();

    // Pattern 1: S01E05 or S01.E05 or S01E05-E06 or S01E05-06
    if let Some(caps) = SXX_EXX_RE.captures(&upper) {
        let s: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
        let ep_start: u32 = caps.get(2).unwrap().as_str().parse().unwrap();
        let ep_end: u32 = caps
            .get(3)
            .map_or(ep_start, |m| m.as_str().parse().unwrap());
        return Some(ParsedEpisode {
            season: Some((s, s)),
            ep_start,
            ep_end,
        });
    }

    // Pattern 2: 1x05 or 1x05-06 (ignore video resolutions like 1920x1080 or 1280x720)
    for caps in AX_B_RE.captures_iter(&upper) {
        let s: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
        if s < 30 {
            let ep_start: u32 = caps.get(2).unwrap().as_str().parse().unwrap();
            let ep_end: u32 = caps
                .get(3)
                .map_or(ep_start, |m| m.as_str().parse().unwrap());
            return Some(ParsedEpisode {
                season: Some((s, s)),
                ep_start,
                ep_end,
            });
        }
    }

    // Pattern 3: Episode markers like E05, EP05, E05_Title, Episode 5
    if let Some(caps) = EP_RE.captures(&upper) {
        let ep_start: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
        let ep_end: u32 = caps
            .get(2)
            .map_or(ep_start, |m| m.as_str().parse().unwrap());
        return Some(ParsedEpisode {
            season: None,
            ep_start,
            ep_end,
        });
    }

    // Pattern 4: Bare episode numbers in series filenames (e.g. "02 - Warriors", "Show - 02 [1080p]", "[02]")
    for re in [
        &*BARE_LEADING_EP_RE,
        &*BARE_DASH_EP_RE,
        &*BARE_BRACKET_EP_RE,
    ] {
        for caps in re.captures_iter(&upper) {
            let ep_start: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
            let ep_end: u32 = caps
                .get(2)
                .map_or(ep_start, |m| m.as_str().parse().unwrap());
            if !is_likely_non_episode_number(ep_start) && !is_likely_non_episode_number(ep_end) {
                return Some(ParsedEpisode {
                    season: None,
                    ep_start,
                    ep_end,
                });
            }
        }
    }

    None
}

fn collect_sxx_exx_captures<'a>(upper: &'a str) -> Vec<regex::Captures<'a>> {
    let mut matches = Vec::new();
    let mut pos = 0;
    while let Some(caps) = SXX_EXX_RE.captures_at(upper, pos) {
        let m = caps.get(0).unwrap();
        let next_pos = match upper[m.start()..m.end()].chars().next_back() {
            Some(ch) if !ch.is_ascii_alphanumeric() => {
                let candidate = m.end() - ch.len_utf8();
                if candidate > m.start() {
                    candidate
                } else {
                    m.end()
                }
            }
            _ => m.end(),
        };
        matches.push(caps);
        if next_pos <= pos {
            break;
        }
        pos = next_pos;
    }
    matches
}

fn extract_seasons_from_segment(segment: &str) -> Vec<(u32, u32)> {
    let upper = segment.to_uppercase();
    let mut ranges: Vec<(u32, u32)> = SEASON_RE
        .captures_iter(&upper)
        .map(|caps| {
            let s_start: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
            let s_end: u32 = caps.get(2).map_or(s_start, |m| m.as_str().parse().unwrap());
            (s_start.min(s_end), s_start.max(s_end))
        })
        .collect();

    if !ranges.is_empty() {
        return ranges;
    }

    ranges = collect_sxx_exx_captures(&upper)
        .into_iter()
        .map(|caps| {
            let s: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
            (s, s)
        })
        .collect();

    if !ranges.is_empty() {
        return ranges;
    }

    AX_B_RE
        .captures_iter(&upper)
        .filter_map(|caps| {
            let s: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
            (s < 30).then_some((s, s))
        })
        .collect()
}

fn extract_season_from_parents(parents: &[&str]) -> Option<Vec<(u32, u32)>> {
    for segment in parents.iter().rev() {
        let ranges = extract_seasons_from_segment(segment);
        if !ranges.is_empty() {
            return Some(ranges);
        }
    }
    None
}

fn parent_segments_reject_season(parents: &[&str], expected_season: u32) -> bool {
    match extract_season_from_parents(parents) {
        Some(ranges) => !ranges
            .iter()
            .any(|&(s_start, s_end)| expected_season >= s_start && expected_season <= s_end),
        None => false,
    }
}

pub fn parse_file_episode(file_path: &str) -> Option<EpisodeInfo> {
    let segments: Vec<&str> = file_path
        .split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .collect();
    if segments.is_empty() {
        return None;
    }

    let (basename, parents) = (
        segments[segments.len() - 1],
        &segments[..segments.len() - 1],
    );
    if let Some(parsed) = extract_episode_from_segment(basename) {
        let season_ranges = parsed
            .season
            .map(|r| vec![r])
            .or_else(|| extract_season_from_parents(parents));
        return Some(EpisodeInfo {
            season_ranges,
            ep_start: parsed.ep_start,
            ep_end: parsed.ep_end,
            from_basename: true,
        });
    }

    if let Some(&immediate_parent) = parents.last()
        && let Some(parsed) = extract_episode_from_segment(immediate_parent)
        && parsed.ep_start == parsed.ep_end
    {
        let grandparent_slice = &parents[..parents.len() - 1];
        let season_ranges = parsed
            .season
            .map(|r| vec![r])
            .or_else(|| extract_season_from_parents(grandparent_slice));
        return Some(EpisodeInfo {
            season_ranges,
            ep_start: parsed.ep_start,
            ep_end: parsed.ep_end,
            from_basename: false,
        });
    }

    None
}

pub fn is_definitely_wrong_episode(
    title: &str,
    expected_season: u32,
    expected_episode: u32,
) -> bool {
    let segments: Vec<&str> = title.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return false;
    }

    // Inspect from the filename (basename) upward so parent folder ranges (e.g. "Show.S01E01-E08/")
    // never mask the episode's own tag in the filename.
    for idx in (0..segments.len()).rev() {
        if let Some(parsed) = extract_episode_from_segment(segments[idx]) {
            if let Some((s_start, s_end)) = parsed.season {
                if expected_season < s_start || expected_season > s_end {
                    return true;
                }
            } else if idx > 0 && parent_segments_reject_season(&segments[..idx], expected_season) {
                return true;
            }

            return !(expected_episode >= parsed.ep_start && expected_episode <= parsed.ep_end);
        }
    }

    false
}

pub fn is_definitely_not_full_season(title: &str, expected_season: u32) -> bool {
    let upper = title.to_uppercase();

    let sxx_matches = collect_sxx_exx_captures(&upper);
    if sxx_matches.len() == 1 {
        let caps = &sxx_matches[0];
        let s: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
        if s != expected_season {
            return true;
        }
        let e_start: u32 = caps.get(2).unwrap().as_str().parse().unwrap();
        let e_end: u32 = caps.get(3).map_or(e_start, |m| m.as_str().parse().unwrap());
        if e_start == e_end {
            return true;
        }
    } else if sxx_matches.len() > 1 {
        let mut min_s = u32::MAX;
        let mut max_s = 0;
        for caps in &sxx_matches {
            let s: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
            min_s = min_s.min(s);
            max_s = max_s.max(s);
        }
        if expected_season < min_s || expected_season > max_s {
            return true;
        }
    }

    for caps in AX_B_RE.captures_iter(&upper) {
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

/// Returns true if a torrent name contains explicit season or episode markers.
pub fn looks_like_series_name(title: &str) -> bool {
    let upper = title.to_uppercase();
    SXX_EXX_RE.is_match(&upper)
        || SEASON_RE.is_match(&upper)
        || EP_RE.is_match(&upper)
        || AX_B_RE.captures_iter(&upper).any(|caps| {
            caps.get(1)
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .is_some_and(|s| s < 30)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_episodes_in_flat_and_nested_season_packs() {
        let avatar_files = [
            "Avatar.The.Last.Airbender.S01.COMPLETE.1080p.NF.WEB-DL.DDP5.1.x264-LiTTLEBLUEMAN[TGx]/Avatar.The.Last.Airbender.S01E01.Aang.1080p.NF.WEB-DL.DDP5.1.x264-NTb.mkv",
            "Avatar.The.Last.Airbender.S01.COMPLETE.1080p.NF.WEB-DL.DDP5.1.x264-LiTTLEBLUEMAN[TGx]/Avatar.The.Last.Airbender.S01E02.Warriors.1080p.NF.WEB-DL.DDP5.1.x264-NTb.mkv",
            "Avatar.The.Last.Airbender.S01.COMPLETE.1080p.NF.WEB-DL.DDP5.1.x264-LiTTLEBLUEMAN[TGx]/Avatar.The.Last.Airbender.S01E03.Omashu.1080p.NF.WEB-DL.DDP5.1.x264-NTb.mkv",
        ];

        assert!(is_definitely_wrong_episode(avatar_files[0], 1, 2));
        assert!(!is_definitely_wrong_episode(avatar_files[1], 1, 2));
        assert!(is_definitely_wrong_episode(avatar_files[2], 1, 2));

        let bocchi_files = [
            "Bocchi.The.Rock.S01.1080p.BluRay.AV1.Opus.2.0-dAV1nci/Bocchi.The.Rock.S01E01.1080p.BluRay.AV1.Opus.2.0-dAV1nci.mkv",
            "Bocchi.The.Rock.S01.1080p.BluRay.AV1.Opus.2.0-dAV1nci/Bocchi.The.Rock.S01E02.1080p.BluRay.AV1.Opus.2.0-dAV1nci.mkv",
            "Bocchi.The.Rock.S01.1080p.BluRay.AV1.Opus.2.0-dAV1nci/Bocchi.The.Rock.S01E12.1080p.BluRay.AV1.Opus.2.0-dAV1nci.mkv",
        ];

        assert!(is_definitely_wrong_episode(bocchi_files[0], 1, 2));
        assert!(!is_definitely_wrong_episode(bocchi_files[1], 1, 2));
        assert!(is_definitely_wrong_episode(bocchi_files[2], 1, 2));
    }

    #[test]
    fn folder_range_does_not_shadow_filename_episode() {
        assert!(is_definitely_wrong_episode(
            "Show.S01E01-E08/Show.S01E01.1080p.mkv",
            1,
            2
        ));
        assert!(!is_definitely_wrong_episode(
            "Show.S01E01-E08/Show.S01E02.1080p.mkv",
            1,
            2
        ));
    }

    #[test]
    fn supports_separator_and_bare_episode_formats() {
        assert!(!is_definitely_wrong_episode("Show.S01.E02.1080p.mkv", 1, 2));
        assert!(is_definitely_wrong_episode("Show.S01.E01.1080p.mkv", 1, 2));

        assert!(!is_definitely_wrong_episode("Show.1x02.1080p.mkv", 1, 2));
        assert!(is_definitely_wrong_episode("Show.1x01.1080p.mkv", 1, 2));

        assert!(!is_definitely_wrong_episode("E02_Warriors.mkv", 1, 2));
        assert!(is_definitely_wrong_episode("E01_Aang.mkv", 1, 2));
        assert!(!is_definitely_wrong_episode("Show.E01-E03.mkv", 1, 2));
        assert!(is_definitely_wrong_episode("Show.E01-E03.mkv", 1, 4));

        assert!(!is_definitely_wrong_episode("02 - Warriors.mkv", 1, 2));
        assert!(is_definitely_wrong_episode("01 - Aang.mkv", 1, 2));

        assert!(!is_definitely_wrong_episode(
            "[SubsPlease] Bocchi the Rock! - 02 (1080p) [ABCD1234].mkv",
            1,
            2
        ));
        assert!(is_definitely_wrong_episode(
            "[SubsPlease] Bocchi the Rock! - 01 (1080p) [ABCD1234].mkv",
            1,
            2
        ));

        assert!(!is_definitely_wrong_episode("Show - 02 [1080p].mkv", 1, 2));
        assert!(is_definitely_wrong_episode("Show - 03 [1080p].mkv", 1, 2));

        // Parent season mismatch should still reject bare-numbered episode files
        assert!(is_definitely_wrong_episode(
            "Season 2/02 - Episode.mkv",
            1,
            2
        ));
    }

    #[test]
    fn keeps_season_packs_for_full_season_filter() {
        assert!(!is_definitely_not_full_season(
            "Avatar.The.Last.Airbender.S01.COMPLETE.1080p.NF.WEB-DL.DDP5.1.x264-LiTTLEBLUEMAN[TGx]",
            1
        ));
        assert!(!is_definitely_not_full_season(
            "Bocchi.The.Rock.S01.1080p.BluRay.AV1.Opus.2.0-dAV1nci",
            1
        ));
        assert!(!is_definitely_not_full_season(
            "Avatar: The Last Airbender - S01 to S03 - 1080p - BR 5.1 X264-RG",
            2
        ));
        assert!(is_definitely_not_full_season(
            "Avatar.The.Last.Airbender.S01E02.Warriors.1080p.mkv",
            1
        ));
        assert!(!is_definitely_not_full_season(
            "Show.S01E01-S01E12.1080p.WEB-DL",
            1
        ));
        assert!(is_definitely_not_full_season(
            "Show.S01E01-S01E12.1080p.WEB-DL",
            2
        ));
        assert!(!is_definitely_not_full_season(
            "Show.S01E01-S02E10.1080p.WEB-DL",
            2
        ));
        assert!(is_definitely_not_full_season(
            "Show.S01E01-S02E10.1080p.WEB-DL",
            3
        ));
    }

    #[test]
    fn inner_season_folder_overrides_outer_multi_season_folder() {
        assert!(is_definitely_wrong_episode(
            "Show.S01-S03/Season 2/02 - Warriors.mkv",
            1,
            2
        ));
        assert!(!is_definitely_wrong_episode(
            "Show.S01-S03/Season 2/02 - Warriors.mkv",
            2,
            2
        ));

        assert_eq!(
            parse_file_episode("Show.S01-S03/Season 2/02 - Warriors.mkv"),
            Some(EpisodeInfo {
                season_ranges: Some(vec![(2, 2)]),
                ep_start: 2,
                ep_end: 2,
                from_basename: true,
            })
        );
    }

    #[test]
    fn parses_file_episode_variants_and_rejects_unnumbered_extras() {
        assert_eq!(
            parse_file_episode("Show.S01/Show.S01E02.1080p.mkv"),
            Some(EpisodeInfo {
                season_ranges: Some(vec![(1, 1)]),
                ep_start: 2,
                ep_end: 2,
                from_basename: true,
            })
        );
        assert_eq!(
            parse_file_episode("Show.1x02.1080p.mkv"),
            Some(EpisodeInfo {
                season_ranges: Some(vec![(1, 1)]),
                ep_start: 2,
                ep_end: 2,
                from_basename: true,
            })
        );
        assert_eq!(
            parse_file_episode("E02_Warriors.mkv"),
            Some(EpisodeInfo {
                season_ranges: None,
                ep_start: 2,
                ep_end: 2,
                from_basename: true,
            })
        );
        assert_eq!(
            parse_file_episode("02 - Warriors.mkv"),
            Some(EpisodeInfo {
                season_ranges: None,
                ep_start: 2,
                ep_end: 2,
                from_basename: true,
            })
        );
        assert_eq!(
            parse_file_episode("[SubsPlease] Bocchi the Rock! - 02 (1080p) [ABCD1234].mkv"),
            Some(EpisodeInfo {
                season_ranges: None,
                ep_start: 2,
                ep_end: 2,
                from_basename: true,
            })
        );
        assert_eq!(
            parse_file_episode("Show.S01E01-E02.1080p.mkv"),
            Some(EpisodeInfo {
                season_ranges: Some(vec![(1, 1)]),
                ep_start: 1,
                ep_end: 2,
                from_basename: true,
            })
        );
        assert_eq!(
            parse_file_episode("Show.S01E02/movie.mkv"),
            Some(EpisodeInfo {
                season_ranges: Some(vec![(1, 1)]),
                ep_start: 2,
                ep_end: 2,
                from_basename: false,
            })
        );
        assert_eq!(parse_file_episode("Extras/Trailer.mkv"), None);
        assert_eq!(parse_file_episode("Show.S01E01-E08/Bonus.mkv"), None);
    }
}
