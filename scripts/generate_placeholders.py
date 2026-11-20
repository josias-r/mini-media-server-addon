#!/usr/bin/env python3
"""Generate 7-segment closed-GOP fMP4 placeholder assets for live splicing.

Usage:
  Regenerate all entries defined in placeholders/placeholders.yaml:
    python3 scripts/generate_placeholders.py

  Regenerate selected entries from the YAML config:
    python3 scripts/generate_placeholders.py --only buffering_0 ready

  Generate a single custom placeholder folder:
    python3 scripts/generate_placeholders.py --title "Hello" --subtitle "World"
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

VIDEO_SAMPLES_PER_SEG = 64
AUDIO_SAMPLES_PER_SEG = 100
VIDEO_SEG_TICKS = 32768   # 64 * (15360 / 30)
AUDIO_SEG_TICKS = 102400  # 100 * 1024
SEGMENTS_PER_STATE = 7

ID_RE = re.compile(r"^[A-Za-z0-9_-]+$")

TITLE_FONT_CANDIDATES = [
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSans-Bold.ttf",
]

SUB_FONT_CANDIDATES = [
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
]


def repo_root() -> Path:
    return Path(__file__).resolve().parent.parent


def find_font(override: str | None, candidates: list[str], label: str) -> str:
    if override:
        if not Path(override).is_file():
            raise FileNotFoundError(f"{label} font not found: {override}")
        return override
    for candidate in candidates:
        if Path(candidate).is_file():
            return candidate
    raise FileNotFoundError(
        f"Could not find a {label} font; pass --{label}-font PATH explicitly."
    )


def slugify(text: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "_", text.strip().lower()).strip("_")
    return slug or "placeholder"


def strip_inline_comment(line: str) -> str:
    in_single = False
    in_double = False
    escaped = False
    for idx, ch in enumerate(line):
        if escaped:
            escaped = False
            continue
        if in_double and ch == "\\":
            escaped = True
            continue
        if ch == "'" and not in_double:
            in_single = not in_single
        elif ch == '"' and not in_single:
            in_double = not in_double
        elif ch == "#" and not in_single and not in_double:
            if idx == 0 or line[idx - 1].isspace():
                return line[:idx]
    return line


def parse_yaml_scalar(raw: str) -> str:
    val = raw.strip()
    if len(val) >= 2 and val[0] == "'" and val[-1] == "'":
        return val[1:-1].replace("''", "'")
    if len(val) >= 2 and val[0] == '"' and val[-1] == '"':
        inner = val[1:-1]
        out: list[str] = []
        i = 0
        while i < len(inner):
            ch = inner[i]
            if ch == "\\" and i + 1 < len(inner):
                nxt = inner[i + 1]
                escapes = {"n": "\n", "t": "\t", "r": "\r", "\\": "\\", '"': '"'}
                out.append(escapes.get(nxt, nxt))
                i += 2
            else:
                out.append(ch)
                i += 1
        return "".join(out)
    return val


def parse_placeholders_yaml(path: Path) -> list[tuple[str, str, str]]:
    """Parse top-level id -> {title, sub|subtitle} mappings from YAML without external deps."""
    entries: dict[str, dict[str, str]] = {}
    order: list[str] = []
    current_id: str | None = None

    for lineno, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        cleaned = strip_inline_comment(raw_line).rstrip()
        if not cleaned.strip():
            continue

        indent = len(cleaned) - len(cleaned.lstrip(" "))
        stripped = cleaned.strip()

        if indent == 0:
            if not stripped.endswith(":"):
                raise ValueError(f"{path}:{lineno}: expected top-level mapping `<id>:`, got {stripped!r}")
            entry_id = parse_yaml_scalar(stripped[:-1])
            if not ID_RE.match(entry_id):
                raise ValueError(
                    f"{path}:{lineno}: invalid id {entry_id!r}; only [A-Za-z0-9_-] allowed"
                )
            if entry_id not in entries:
                entries[entry_id] = {}
                order.append(entry_id)
            current_id = entry_id
        else:
            if current_id is None:
                raise ValueError(f"{path}:{lineno}: indented field before any top-level id")
            if ":" not in stripped:
                raise ValueError(f"{path}:{lineno}: expected `key: value`, got {stripped!r}")
            key, rest = stripped.split(":", 1)
            key = key.strip()
            value = parse_yaml_scalar(rest)
            if key in ("title", "sub", "subtitle"):
                norm_key = "sub" if key == "subtitle" else key
                entries[current_id][norm_key] = value
            else:
                raise ValueError(f"{path}:{lineno}: unsupported field {key!r} under {current_id!r}")

    result: list[tuple[str, str, str]] = []
    for entry_id in order:
        fields = entries[entry_id]
        if "title" not in fields:
            raise ValueError(f"{path}: entry {entry_id!r} is missing required `title`")
        result.append((entry_id, fields["title"], fields.get("sub", "")))
    return result


def parse_boxes(data: bytes | bytearray, start: int = 0, end: int | None = None) -> list[tuple[str, int, int]]:
    if end is None:
        end = len(data)
    pos = start
    boxes: list[tuple[str, int, int]] = []
    while pos + 8 <= end:
        sz = struct.unpack(">I", data[pos : pos + 4])[0]
        typ = data[pos + 4 : pos + 8].decode("latin1")
        if sz == 0:
            sz = end - pos
        elif sz < 8 or pos + sz > end:
            raise ValueError(f"Invalid MP4 box {typ!r} of size {sz} at offset {pos}")
        boxes.append((typ, pos, sz))
        pos += sz
    if pos != end:
        raise ValueError(f"Trailing {end - pos} bytes in MP4 container")
    return boxes


def normalize_segment(seg_bytes: bytes, seg_idx: int) -> bytes:
    """Validate sample counts and normalize sequence_number & tfdt inside a [moof][mdat] segment."""
    buf = bytearray(seg_bytes)
    top = parse_boxes(buf)
    if len(top) != 2 or top[0][0] != "moof" or top[1][0] != "mdat":
        raise ValueError(f"Expected [moof, mdat], got {[b[0] for b in top]}")

    moof_sz = top[0][2]
    saw_mfhd = False
    seen_tracks: set[int] = set()

    for ctyp, cpos, csz in parse_boxes(buf, 8, moof_sz):
        if ctyp == "mfhd":
            struct.pack_into(">I", buf, cpos + 12, seg_idx + 1)
            saw_mfhd = True
        elif ctyp == "traf":
            track_id: int | None = None
            tfdt_pos: int | None = None
            sample_count: int | None = None
            for ttyp, tpos, _ in parse_boxes(buf, cpos + 8, cpos + csz):
                if ttyp == "tfhd":
                    track_id = struct.unpack(">I", buf[tpos + 12 : tpos + 16])[0]
                elif ttyp == "tfdt":
                    tfdt_pos = tpos
                elif ttyp == "trun":
                    sample_count = struct.unpack(">I", buf[tpos + 12 : tpos + 16])[0]

            if track_id not in (1, 2) or tfdt_pos is None or sample_count is None:
                raise ValueError(
                    f"Malformed traf in segment {seg_idx}: track={track_id}, tfdt={tfdt_pos}, samples={sample_count}"
                )

            expected_samples = VIDEO_SAMPLES_PER_SEG if track_id == 1 else AUDIO_SAMPLES_PER_SEG
            if sample_count != expected_samples:
                raise ValueError(
                    f"Segment {seg_idx} track {track_id}: expected {expected_samples} samples, got {sample_count}"
                )

            tfdt_val = seg_idx * (VIDEO_SEG_TICKS if track_id == 1 else AUDIO_SEG_TICKS)
            ver = buf[tfdt_pos + 8]
            if ver == 1:
                struct.pack_into(">Q", buf, tfdt_pos + 12, tfdt_val)
            elif ver == 0:
                struct.pack_into(">I", buf, tfdt_pos + 12, tfdt_val)
            else:
                raise ValueError(f"Unsupported tfdt version {ver}")

            seen_tracks.add(track_id)

    if not saw_mfhd or seen_tracks != {1, 2}:
        raise ValueError(f"Segment {seg_idx} missing mfhd or tracks: tracks={seen_tracks}")

    return bytes(buf)


def escape_ffmpeg_filter_path(path: str) -> str:
    return path.replace("\\", "\\\\").replace(":", "\\:").replace("'", "\\'")


def generate_state_segments(
    input_mp4: Path,
    title: str,
    subtitle: str,
    title_font: str,
    sub_font: str,
    crf: int,
    audio_bitrate: str,
) -> tuple[bytes, list[bytes]]:
    with tempfile.TemporaryDirectory() as tmpdir:
        tmp = Path(tmpdir)
        title_file = tmp / "title.txt"
        sub_file = tmp / "sub.txt"
        raw_mp4 = tmp / "raw.mp4"

        title_file.write_text(title, encoding="utf-8")
        sub_file.write_text(subtitle, encoding="utf-8")

        title_y = 78 if subtitle else 95
        vf_parts = [
            "fps=30",
            "setpts=N/(30*TB)",
            "drawbox=x=24:y=66:w=432:h=82:color=black@0.65:t=fill",
            (
                f"drawtext=fontfile='{escape_ffmpeg_filter_path(title_font)}':"
                f"textfile='{escape_ffmpeg_filter_path(str(title_file))}':"
                f"expansion=none:fontcolor=white:fontsize=24:x=(w-text_w)/2:y={title_y}"
            ),
        ]
        if subtitle:
            vf_parts.append(
                f"drawtext=fontfile='{escape_ffmpeg_filter_path(sub_font)}':"
                f"textfile='{escape_ffmpeg_filter_path(str(sub_file))}':"
                "expansion=none:fontcolor=white@0.92:fontsize=17:x=(w-text_w)/2:y=114"
            )

        cmd = [
            "ffmpeg",
            "-y",
            "-v",
            "error",
            "-stream_loop",
            "2",
            "-i",
            str(input_mp4),
            "-vf",
            ",".join(vf_parts),
            "-af",
            "aresample=48000,asetpts=N/SR/TB",
            "-frames:v",
            "512",
            "-r",
            "30",
            "-video_track_timescale",
            "15360",
            "-c:v",
            "libx264",
            "-preset",
            "slow",
            "-crf",
            str(crf),
            "-profile:v",
            "main",
            "-level",
            "3.1",
            "-pix_fmt",
            "yuv420p",
            "-g",
            "64",
            "-keyint_min",
            "64",
            "-sc_threshold",
            "0",
            "-bf",
            "0",
            "-c:a",
            "aac",
            "-b:a",
            audio_bitrate,
            "-ar",
            "48000",
            "-ac",
            "2",
            "-frames:a",
            "801",
            "-movflags",
            "+frag_keyframe+empty_moov+default_base_moof+omit_tfhd_offset",
            str(raw_mp4),
        ]
        subprocess.run(cmd, check=True)
        data = raw_mp4.read_bytes()

    boxes = parse_boxes(data)
    if len(boxes) < 4 or boxes[0][0] != "ftyp" or boxes[1][0] != "moov":
        raise ValueError(f"Unexpected init boxes: {[b[0] for b in boxes[:4]]}")

    init_bytes = data[boxes[0][1] : boxes[1][1] + boxes[1][2]]

    fragments: list[bytes] = []
    idx = 2
    while idx < len(boxes):
        typ, pos, sz = boxes[idx]
        if typ == "moof":
            if idx + 1 >= len(boxes) or boxes[idx + 1][0] != "mdat":
                raise ValueError("moof without following mdat")
            mdat_pos, mdat_sz = boxes[idx + 1][1], boxes[idx + 1][2]
            fragments.append(data[pos : mdat_pos + mdat_sz])
            idx += 2
        elif typ == "mfra":
            idx += 1
        else:
            raise ValueError(f"Unexpected top-level box {typ!r} at index {idx}")

    if len(fragments) != SEGMENTS_PER_STATE + 1:
        raise ValueError(
            f"Expected {SEGMENTS_PER_STATE + 1} fragments (1 warmup + {SEGMENTS_PER_STATE}), got {len(fragments)}"
        )

    # Discard fragment 0 (AAC encoder priming warmup) and normalize fragments 1..7.
    normalized = [normalize_segment(frag, i) for i, frag in enumerate(fragments[1:])]
    return init_bytes, normalized


def main() -> int:
    root = repo_root()
    parser = argparse.ArgumentParser(
        description="Generate 7-segment fMP4 placeholder assets for live streaming."
    )
    parser.add_argument(
        "--config",
        type=Path,
        default=root / "placeholders" / "placeholders.yaml",
        help="YAML config file with id -> {title, sub} entries",
    )
    parser.add_argument(
        "--only",
        nargs="+",
        metavar="ID",
        help="In config mode, regenerate only these state IDs",
    )
    parser.add_argument("--title", help="Single-entry mode: overlay title text")
    parser.add_argument(
        "--subtitle",
        "--sub",
        dest="subtitle",
        default="",
        help="Single-entry mode: overlay subtitle text",
    )
    parser.add_argument(
        "--id",
        dest="entry_id",
        help="Single-entry mode: output subfolder name (default: slugified title)",
    )
    parser.add_argument(
        "--input",
        type=Path,
        default=root / "placeholder.mp4",
        help="Base placeholder MP4 file",
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=root / "placeholders",
        help="Output placeholders directory",
    )
    parser.add_argument("--crf", type=int, default=30, help="x264 CRF value (default: 30)")
    parser.add_argument(
        "--audio-bitrate",
        default="48k",
        help="AAC audio bitrate (default: 48k)",
    )
    parser.add_argument("--title-font", help="Path to bold TrueType/OpenType font")
    parser.add_argument("--sub-font", help="Path to regular TrueType/OpenType font")
    parser.add_argument(
        "--clean",
        action="store_true",
        help="In full config mode, remove subdirectories not listed in config",
    )
    args = parser.parse_args()

    if not args.input.is_file():
        print(f"Error: input file not found: {args.input}", file=sys.stderr)
        return 1
    if shutil.which("ffmpeg") is None:
        print("Error: ffmpeg binary not found in PATH", file=sys.stderr)
        return 1

    title_font = find_font(args.title_font, TITLE_FONT_CANDIDATES, "title")
    sub_font = find_font(args.sub_font, SUB_FONT_CANDIDATES, "sub")

    single_mode = args.title is not None
    if single_mode:
        if args.only:
            print("Error: --only cannot be combined with --title", file=sys.stderr)
            return 1
        entry_id = args.entry_id or slugify(args.title)
        if not ID_RE.match(entry_id):
            print(f"Error: invalid --id {entry_id!r}; only [A-Za-z0-9_-] allowed", file=sys.stderr)
            return 1
        targets = [(entry_id, args.title, args.subtitle)]
        full_config_run = False
    else:
        if not args.config.is_file():
            print(f"Error: config file not found: {args.config}", file=sys.stderr)
            return 1
        all_entries = parse_placeholders_yaml(args.config)
        if args.only:
            by_id = {eid: (eid, t, s) for eid, t, s in all_entries}
            missing = [eid for eid in args.only if eid not in by_id]
            if missing:
                print(
                    f"Error: unknown ID(s) in --only: {', '.join(missing)}",
                    file=sys.stderr,
                )
                return 1
            targets = [by_id[eid] for eid in args.only]
            full_config_run = False
        else:
            targets = all_entries
            full_config_run = True

    args.out_dir.mkdir(parents=True, exist_ok=True)
    init_path = args.out_dir / "init.mp4"
    expected_init: bytes | None = None
    if not full_config_run and init_path.is_file():
        expected_init = init_path.read_bytes()

    total_bytes = 0
    for entry_id, title, subtitle in targets:
        print(f"Generating {entry_id}: {title!r} / {subtitle!r} ...", flush=True)
        init_bytes, segs = generate_state_segments(
            input_mp4=args.input,
            title=title,
            subtitle=subtitle,
            title_font=title_font,
            sub_font=sub_font,
            crf=args.crf,
            audio_bitrate=args.audio_bitrate,
        )

        if expected_init is None:
            expected_init = init_bytes
        elif init_bytes != expected_init:
            raise RuntimeError(
                f"Generated init.mp4 for {entry_id!r} differs from existing/previous init.mp4!"
            )

        state_dir = args.out_dir / entry_id
        state_dir.mkdir(parents=True, exist_ok=True)
        entry_bytes = 0
        for idx, seg in enumerate(segs):
            seg_path = state_dir / f"seg_{idx}.m4s"
            seg_path.write_bytes(seg)
            entry_bytes += len(seg)
        total_bytes += entry_bytes
        print(f"  -> {state_dir} (7 segments, {entry_bytes / 1024:.1f} KiB)", flush=True)

    if expected_init is not None and (full_config_run or not init_path.is_file()):
        init_path.write_bytes(expected_init)
        print(f"Wrote {init_path} ({len(expected_init)} bytes)")

    if full_config_run and args.clean:
        known_ids = {eid for eid, _, _ in targets}
        for child in args.out_dir.iterdir():
            if child.is_dir() and child.name not in known_ids:
                print(f"Removing stale directory {child}")
                shutil.rmtree(child)

    print(
        f"Done! Generated {len(targets)} state(s), segment payload total = {total_bytes / (1024 * 1024):.2f} MiB."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
