use crate::parser::{
    is_definitely_not_full_season, is_definitely_wrong_episode, is_video_file, parse_file_episode,
};
use std::collections::HashSet;
use strsim::levenshtein;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextEpisodeMatch {
    pub file_vec_idx: usize,
    pub season: u32,
    pub episode: u32,
}

fn split_segments(path: &str) -> Vec<&str> {
    path.split(['/', '\\']).filter(|s| !s.is_empty()).collect()
}

fn parent_dir_key(path: &str) -> String {
    let segs = split_segments(path);
    if segs.len() <= 1 {
        return String::new();
    }
    segs[..segs.len() - 1].join("/").to_lowercase()
}

fn normalize_alnum_lower(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn tokenize_stem(basename: &str) -> HashSet<String> {
    let stem = basename.rsplit_once('.').map_or(basename, |(s, _)| s);
    stem.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn is_extra_or_sample_relative_to(
    candidate_name: &str,
    candidate_size: u64,
    current_name: &str,
    current_size: u64,
) -> bool {
    let cand_segs = split_segments(candidate_name);
    let cur_segs = split_segments(current_name);

    if cand_segs.is_empty() {
        return true;
    }

    let cur_parents_norm: HashSet<String> = if cur_segs.len() > 1 {
        cur_segs[..cur_segs.len() - 1]
            .iter()
            .map(|s| normalize_alnum_lower(s))
            .collect()
    } else {
        HashSet::new()
    };

    if cand_segs.len() > 1 {
        for seg in &cand_segs[..cand_segs.len() - 1] {
            let norm = normalize_alnum_lower(seg);
            if !cur_parents_norm.contains(&norm)
                && matches!(
                    norm.as_str(),
                    "sample"
                        | "samples"
                        | "extra"
                        | "extras"
                        | "bonus"
                        | "bonuses"
                        | "featurette"
                        | "featurettes"
                        | "special"
                        | "specials"
                        | "sp"
                        | "sps"
                        | "ova"
                        | "ovas"
                        | "oad"
                        | "oads"
                        | "ncop"
                        | "nced"
                        | "pv"
                        | "pvs"
                        | "trailer"
                        | "trailers"
                        | "promo"
                        | "promos"
                        | "preview"
                        | "previews"
                        | "menu"
                        | "menus"
                        | "scans"
                        | "ost"
                        | "cd"
                        | "cds"
                        | "creditless"
                )
            {
                return true;
            }
        }
    }

    let cand_basename = cand_segs[cand_segs.len() - 1];
    let cur_basename = cur_segs.last().copied().unwrap_or("");
    let cand_tokens = tokenize_stem(cand_basename);
    let cur_tokens = tokenize_stem(cur_basename);

    for token in &cand_tokens {
        if cur_tokens.contains(token) {
            continue;
        }
        if matches!(token.as_str(), "ncop" | "nced" | "creditless") {
            return true;
        }
        if matches!(
            token.as_str(),
            "sample" | "trailer" | "teaser" | "preview" | "promo" | "featurette" | "menu"
        ) && current_size > 0
            && candidate_size.saturating_mul(4) < current_size
        {
            return true;
        }
    }

    false
}

pub fn find_next_episode_file(
    files: &[(&str, u64)],
    torrent_name: Option<&str>,
    current_file_vec_idx: usize,
    season: u32,
    episode: u32,
) -> Option<NextEpisodeMatch> {
    if current_file_vec_idx >= files.len() {
        return None;
    }

    let video_count = files
        .iter()
        .filter(|&&(name, _)| is_video_file(name))
        .count();
    if video_count < 2 {
        return None;
    }

    if let Some(tname) = torrent_name.map(str::trim).filter(|s| !s.is_empty())
        && is_definitely_not_full_season(tname, season)
    {
        return None;
    }

    let (current_name, current_size) = files[current_file_vec_idx];
    let current_parent_key = parent_dir_key(current_name);
    let current_info = parse_file_episode(current_name);

    let next_episode = if let Some(info) = current_info
        && info.from_basename
        && episode >= info.ep_start
        && episode <= info.ep_end
        && info.ep_end.saturating_sub(info.ep_start) <= 3
    {
        info.ep_end.checked_add(1)?
    } else {
        episode.checked_add(1)?
    };

    let find_candidate = |target_season: u32,
                          target_episode: u32,
                          require_explicit_season: bool|
     -> Option<NextEpisodeMatch> {
        let mut best: Option<((bool, bool, u64), usize)> = None;

        for (idx, &(name, size)) in files.iter().enumerate() {
            if idx == current_file_vec_idx || !is_video_file(name) {
                continue;
            }
            if is_extra_or_sample_relative_to(name, size, current_name, current_size) {
                continue;
            }
            let Some(info) = parse_file_episode(name) else {
                continue;
            };
            let same_parent = parent_dir_key(name) == current_parent_key;
            if !info.from_basename && same_parent {
                continue;
            }
            match info.season_ranges {
                Some(ref ranges) => {
                    if !ranges
                        .iter()
                        .any(|&(s, e)| target_season >= s && target_season <= e)
                    {
                        continue;
                    }
                }
                None => {
                    if require_explicit_season {
                        continue;
                    }
                }
            }
            if target_episode < info.ep_start || target_episode > info.ep_end {
                continue;
            }

            let score = (info.from_basename, same_parent, size);
            if best.is_none_or(|(best_score, _)| score > best_score) {
                best = Some((score, idx));
            }
        }

        best.map(|(_, file_vec_idx)| NextEpisodeMatch {
            file_vec_idx,
            season: target_season,
            episode: target_episode,
        })
    };

    if let Some(matched) = find_candidate(season, next_episode, false) {
        return Some(matched);
    }

    if season >= 1
        && let Some(next_season) = season.checked_add(1)
    {
        let has_later_in_current_season = files.iter().enumerate().any(|(idx, &(name, size))| {
            if idx == current_file_vec_idx || !is_video_file(name) {
                return false;
            }
            if is_extra_or_sample_relative_to(name, size, current_name, current_size) {
                return false;
            }
            let Some(info) = parse_file_episode(name) else {
                return false;
            };
            let same_parent = parent_dir_key(name) == current_parent_key;
            if !info.from_basename && same_parent {
                return false;
            }
            let in_current_season = match info.season_ranges {
                Some(ref ranges) => ranges.iter().any(|&(s, e)| season >= s && season <= e),
                None => true,
            };
            in_current_season && info.ep_end >= next_episode
        });

        if !has_later_in_current_season {
            return find_candidate(next_season, 1, true);
        }
    }

    None
}

pub fn find_best_file_match(
    files: &[(&str, u64)],
    expected_title: &str,
    is_series: bool,
    season: u32,
    episode: u32,
) -> Option<usize> {
    // 1. Filter out non-videos and wrong episodes
    let mut valid_files = Vec::new();
    for (idx, &(name, size)) in files.iter().enumerate() {
        if is_video_file(name) {
            if is_series && is_definitely_wrong_episode(name, season, episode) {
                continue;
            }
            valid_files.push((idx, name, size));
        }
    }

    // Fallback: If no files passed the episode filter, just take all videos
    if valid_files.is_empty() {
        for (idx, &(name, size)) in files.iter().enumerate() {
            if is_video_file(name) {
                valid_files.push((idx, name, size));
            }
        }
    }

    if valid_files.is_empty() {
        return None;
    }

    // 2. Token + Levenshtein Matching
    let expected_title_lower = expected_title.to_lowercase();
    let expected_tokens: Vec<&str> = expected_title_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();

    let mut best_idx = 0;
    // We want the LOWEST distance score. And for ties, the LARGEST size.
    // We can store (distance_score, size).
    // To easily use `cmp`, since we want to MINIMIZE distance and MAXIMIZE size,
    // we can sort by distance ascending, then size descending.
    // Or keep a running best.
    let mut best_score: Option<(usize, u64)> = None;

    for (i, &(_idx, name, size)) in valid_files.iter().enumerate() {
        let basename = name.rsplit(['/', '\\']).next().unwrap_or(name);
        let filename_lower = basename.to_lowercase();
        let filename_tokens: Vec<&str> = filename_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();

        let mut total_distance = 0;

        for &expected_token in &expected_tokens {
            // Find the minimum Levenshtein distance from this expected token to any filename token
            let mut min_token_dist = usize::MAX;
            for &file_token in &filename_tokens {
                let dist = levenshtein(expected_token, file_token);
                if dist < min_token_dist {
                    min_token_dist = dist;
                }
            }
            // If filename has no tokens, min_token_dist remains MAX. We should fallback to the expected token length.
            if min_token_dist == usize::MAX {
                min_token_dist = expected_token.len();
            }
            total_distance += min_token_dist;
        }

        let current_score = (total_distance, size);

        match best_score {
            None => {
                best_score = Some(current_score);
                best_idx = i;
            }
            Some((best_dist, best_size)) => {
                // If total_distance is strictly less, it's better.
                // If it's equal, then we check if size is strictly greater.
                if total_distance < best_dist || (total_distance == best_dist && size > best_size) {
                    best_score = Some(current_score);
                    best_idx = i;
                }
            }
        }
    }

    let (chosen_idx, _, _) = valid_files[best_idx];
    Some(chosen_idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_requested_episode_from_avatar_season_pack_despite_smaller_size() {
        let files = vec![
            ("[TGx]Downloaded from torrentgalaxy.to .txt", 479),
            (
                "Avatar.The.Last.Airbender.S01.COMPLETE.1080p.NF.WEB-DL.DDP5.1.x264-LiTTLEBLUEMAN[TGx]/Avatar.The.Last.Airbender.S01E01.Aang.1080p.NF.WEB-DL.DDP5.1.x264-NTb.mkv",
                2_555_000_000,
            ),
            (
                "Avatar.The.Last.Airbender.S01.COMPLETE.1080p.NF.WEB-DL.DDP5.1.x264-LiTTLEBLUEMAN[TGx]/Avatar.The.Last.Airbender.S01E02.Warriors.1080p.NF.WEB-DL.DDP5.1.x264-NTb.mkv",
                1_900_000_000,
            ),
            (
                "Avatar.The.Last.Airbender.S01.COMPLETE.1080p.NF.WEB-DL.DDP5.1.x264-LiTTLEBLUEMAN[TGx]/Avatar.The.Last.Airbender.S01E08.Legends.1080p.NF.WEB-DL.DDP5.1.x264-NTb.mkv",
                3_414_000_000,
            ),
            ("NEW upcoming releases by Xclusive.txt", 71),
        ];

        let matched = find_best_file_match(&files, "Avatar: The Last Airbender S01E02", true, 1, 2);
        assert_eq!(matched, Some(2));
    }

    #[test]
    fn picks_requested_episode_from_bocchi_season_pack() {
        let files = vec![
            (
                "Bocchi.The.Rock.S01.1080p.BluRay.AV1.Opus.2.0-dAV1nci/Bocchi.The.Rock.S01E01.1080p.BluRay.AV1.Opus.2.0-dAV1nci.mkv",
                263_313_996,
            ),
            (
                "Bocchi.The.Rock.S01.1080p.BluRay.AV1.Opus.2.0-dAV1nci/Bocchi.The.Rock.S01E02.1080p.BluRay.AV1.Opus.2.0-dAV1nci.mkv",
                270_033_864,
            ),
            (
                "Bocchi.The.Rock.S01.1080p.BluRay.AV1.Opus.2.0-dAV1nci/Bocchi.The.Rock.S01E03.1080p.BluRay.AV1.Opus.2.0-dAV1nci.mkv",
                314_039_932,
            ),
        ];

        let matched = find_best_file_match(&files, "Bocchi the Rock! S01E02", true, 1, 2);
        assert_eq!(matched, Some(1));
    }

    #[test]
    fn picks_requested_episode_with_bare_numbering_and_folder_ranges() {
        let bare_files = vec![
            ("Show.S01/01 - Pilot.mkv", 500_000_000),
            ("Show.S01/02 - Warriors.mkv", 350_000_000),
            ("Show.S01/03 - Omashu.mkv", 450_000_000),
        ];
        assert_eq!(
            find_best_file_match(&bare_files, "Show S01E02", true, 1, 2),
            Some(1)
        );

        let folder_range_files = vec![
            ("Show.S01E01-E08/Show.S01E01.mkv", 500_000_000),
            ("Show.S01E01-E08/Show.S01E02.mkv", 350_000_000),
        ];
        assert_eq!(
            find_best_file_match(&folder_range_files, "Show S01E02", true, 1, 2),
            Some(1)
        );
    }

    #[test]
    fn finds_next_episode_across_season_pack_conventions() {
        let standard_files = vec![
            ("Show.S01.1080p/Show.S01E01.mkv", 1_000_000_000),
            ("Show.S01.1080p/Show.S01E02.mkv", 1_100_000_000),
            ("Show.S01.1080p/Show.S01E03.mkv", 950_000_000),
        ];
        assert_eq!(
            find_next_episode_file(&standard_files, Some("Show.S01.1080p"), 0, 1, 1),
            Some(NextEpisodeMatch {
                file_vec_idx: 1,
                season: 1,
                episode: 2,
            })
        );

        let subsplease_files = vec![
            (
                "[SubsPlease] Show (01-12)/[SubsPlease] Show - 01 (1080p).mkv",
                400_000_000,
            ),
            (
                "[SubsPlease] Show (01-12)/[SubsPlease] Show - 02 (1080p).mkv",
                420_000_000,
            ),
        ];
        assert_eq!(
            find_next_episode_file(
                &subsplease_files,
                Some("[SubsPlease] Show (01-12)"),
                0,
                1,
                1
            ),
            Some(NextEpisodeMatch {
                file_vec_idx: 1,
                season: 1,
                episode: 2,
            })
        );

        let bare_files = vec![
            ("Show.S01/01 - Pilot.mkv", 500_000_000),
            ("Show.S01/02 - Warriors.mkv", 450_000_000),
        ];
        assert_eq!(
            find_next_episode_file(&bare_files, Some("Show.S01"), 0, 1, 1),
            Some(NextEpisodeMatch {
                file_vec_idx: 1,
                season: 1,
                episode: 2,
            })
        );

        let ax_b_files = vec![
            ("Show.1x01.mkv", 500_000_000),
            ("Show.1x02.mkv", 500_000_000),
        ];
        assert_eq!(
            find_next_episode_file(&ax_b_files, None, 0, 1, 1),
            Some(NextEpisodeMatch {
                file_vec_idx: 1,
                season: 1,
                episode: 2,
            })
        );

        let ep_files = vec![
            ("E01_Pilot.mkv", 500_000_000),
            ("E02_Next.mkv", 500_000_000),
        ];
        assert_eq!(
            find_next_episode_file(&ep_files, None, 0, 1, 1),
            Some(NextEpisodeMatch {
                file_vec_idx: 1,
                season: 1,
                episode: 2,
            })
        );
    }

    #[test]
    fn finale_returns_none_and_never_falls_back_to_extras_or_wrong_episode() {
        let files = vec![
            ("Show.S01/Show.S01E01.mkv", 1_000_000_000),
            ("Show.S01/Show.S01E02.mkv", 1_000_000_000),
            ("Show.S01/Extras/Trailer.mkv", 50_000_000),
            ("Show.S01/Bonus.mkv", 60_000_000),
        ];
        assert_eq!(
            find_next_episode_file(&files, Some("Show.S01"), 1, 1, 2),
            None
        );
    }

    #[test]
    fn multi_season_pack_handles_finale_transition_and_gaps() {
        let explicit_files = vec![
            ("Show.S01-S02/Show.S01E08.mkv", 1_000_000_000),
            ("Show.S01-S02/Show.S02E01.mkv", 1_000_000_000),
        ];
        assert_eq!(
            find_next_episode_file(&explicit_files, Some("Show.S01-S02"), 0, 1, 8),
            Some(NextEpisodeMatch {
                file_vec_idx: 1,
                season: 2,
                episode: 1,
            })
        );

        let nested_files = vec![
            ("Show.S01-S03/Season 1/08.mkv", 900_000_000),
            ("Show.S01-S03/Season 2/01.mkv", 950_000_000),
        ];
        assert_eq!(
            find_next_episode_file(&nested_files, Some("Show.S01-S03"), 0, 1, 8),
            Some(NextEpisodeMatch {
                file_vec_idx: 1,
                season: 2,
                episode: 1,
            })
        );

        let gap_files = vec![
            ("Show.S01-S02/Show.S01E04.mkv", 1_000_000_000),
            ("Show.S01-S02/Show.S01E06.mkv", 1_000_000_000),
            ("Show.S01-S02/Show.S02E01.mkv", 1_000_000_000),
        ];
        assert_eq!(
            find_next_episode_file(&gap_files, Some("Show.S01-S02"), 0, 1, 4),
            None
        );

        // Flat bare-numbered season pack finale must not wrap back to episode 1 as S02E01
        let flat_bare = vec![
            ("01 - Pilot.mkv", 500_000_000),
            ("08 - Finale.mkv", 500_000_000),
        ];
        assert_eq!(
            find_next_episode_file(&flat_bare, Some("Show.S01"), 1, 1, 8),
            None
        );

        // Season 0 specials finale must not jump to S01E01
        let specials_and_s1 = vec![
            ("Show.S00E02.OVA.mkv", 500_000_000),
            ("Show.S01E01.mkv", 500_000_000),
        ];
        assert_eq!(
            find_next_episode_file(&specials_and_s1, Some("Show.S00-S01"), 0, 0, 2),
            None
        );
    }

    #[test]
    fn double_episode_file_triggers_following_episode() {
        let files = vec![
            ("Show.S01/Show.S01E01-E02.mkv", 2_000_000_000),
            ("Show.S01/Show.S01E03.mkv", 1_000_000_000),
        ];
        assert_eq!(
            find_next_episode_file(&files, Some("Show.S01"), 0, 1, 1),
            Some(NextEpisodeMatch {
                file_vec_idx: 1,
                season: 1,
                episode: 3,
            })
        );
    }

    #[test]
    fn single_episode_torrent_returns_none() {
        let single = vec![("Show.S01E01.1080p.mkv", 1_500_000_000)];
        assert_eq!(
            find_next_episode_file(&single, Some("Show.S01E01.1080p"), 0, 1, 1),
            None
        );

        let with_sample = vec![
            ("Show.S01E01.1080p/Show.S01E01.1080p.mkv", 1_500_000_000),
            ("Show.S01E01.1080p/Sample/sample.mkv", 25_000_000),
        ];
        assert_eq!(
            find_next_episode_file(&with_sample, Some("Show.S01E01.1080p"), 0, 1, 1),
            None
        );
    }

    #[test]
    fn filters_extras_and_samples_while_keeping_legit_titles() {
        let extra_files = vec![
            ("Show.S01/Show.S01E01.mkv", 1_000_000_000),
            ("Show.S01/Extras/NCED - 02.mkv", 120_000_000),
            ("Show.S01/Show.S01E02.Sample.mkv", 30_000_000),
            ("Show.S01/Show.S01E02.mkv", 1_000_000_000),
        ];
        assert_eq!(
            find_next_episode_file(&extra_files, Some("Show.S01"), 0, 1, 1),
            Some(NextEpisodeMatch {
                file_vec_idx: 3,
                season: 1,
                episode: 2,
            })
        );

        let no_real_ep2 = vec![
            ("Show.S01/Show.S01E01.mkv", 1_000_000_000),
            ("Show.S01/Extras/NCED - 02.mkv", 120_000_000),
            ("Show.S01/Show.S01E02.Sample.mkv", 30_000_000),
        ];
        assert_eq!(
            find_next_episode_file(&no_real_ep2, Some("Show.S01"), 0, 1, 1),
            None
        );

        // Show legitimately named "Extras"
        let show_named_extras = vec![
            ("Extras.S01/Extras.S01E01.mkv", 500_000_000),
            ("Extras.S01/Extras.S01E02.mkv", 520_000_000),
        ];
        assert_eq!(
            find_next_episode_file(&show_named_extras, Some("Extras.S01"), 0, 1, 1),
            Some(NextEpisodeMatch {
                file_vec_idx: 1,
                season: 1,
                episode: 2,
            })
        );

        // Full-sized episode titled "The Sample"
        let sample_title_ep = vec![
            ("Show.S01/Show.S01E01.Pilot.mkv", 500_000_000),
            ("Show.S01/Show.S01E02.The.Sample.mkv", 450_000_000),
        ];
        assert_eq!(
            find_next_episode_file(&sample_title_ep, Some("Show.S01"), 0, 1, 1),
            Some(NextEpisodeMatch {
                file_vec_idx: 1,
                season: 1,
                episode: 2,
            })
        );
    }

    #[test]
    fn handles_per_episode_subfolders_and_ignores_range_folder_extras() {
        let per_ep_subfolders = vec![
            ("Show.S01/Show.S01E01/movie.mkv", 1_000_000_000),
            ("Show.S01/Show.S01E02/movie.mkv", 1_050_000_000),
        ];
        assert_eq!(
            find_next_episode_file(&per_ep_subfolders, Some("Show.S01"), 0, 1, 1),
            Some(NextEpisodeMatch {
                file_vec_idx: 1,
                season: 1,
                episode: 2,
            })
        );

        let range_folder_extra = vec![
            ("Show.S01E01-E08/Show.S01E01.mkv", 1_000_000_000),
            ("Show.S01E01-E08/Bonus.mkv", 100_000_000),
        ];
        assert_eq!(
            find_next_episode_file(&range_folder_extra, Some("Show.S01E01-E08"), 0, 1, 1),
            None
        );
    }
}
