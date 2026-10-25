#!/usr/bin/env python3
"""Shift the root key of every sample in an SF2 file without touching the audio data.

Useful for banks that are uniformly one or more octaves low/high: lowering a sample's
`byOriginalPitch` makes it play higher at the same MIDI key, and vice versa. This changes
every preset that uses the sample, so only use it when the whole bank has the same offset.

    python tools/rekey_sf2.py input.sf2 output.sf2 --root-shift -12

`--root-shift -12` lowers every root by 12 semitones, which raises the rendered pitch by
one octave. The output is byte-identical except for the root-key bytes in the `shdr` chunk.
"""
from __future__ import annotations

import argparse
import struct
from pathlib import Path

SHDR_RECORD = 46
ROOT_OFFSET = 40  # byOriginalPitch: 20 name + 4 start + 4 end + 4 loop start + 4 loop end + 4 rate


def find_pdta(data: bytearray) -> tuple[int, int]:
    if data[:4] != b"RIFF" or data[8:12] != b"sfbk":
        raise ValueError("not a RIFF/sfbk SoundFont")
    pos = 12
    while pos + 8 <= len(data):
        cid = bytes(data[pos : pos + 4])
        size = struct.unpack_from("<I", data, pos + 4)[0]
        if cid == b"LIST" and bytes(data[pos + 8 : pos + 12]) == b"pdta":
            return pos + 12, pos + 8 + size
        pos += 8 + size + (size & 1)
    raise ValueError("pdta chunk not found")


def find_shdr(data: bytearray, start: int, end: int) -> tuple[int, int]:
    pos = start
    while pos + 8 <= end:
        cid = bytes(data[pos : pos + 4])
        size = struct.unpack_from("<I", data, pos + 4)[0]
        if cid == b"shdr":
            return pos + 8, size
        pos += 8 + size + (size & 1)
    raise ValueError("shdr chunk not found")


def rekey(src: Path, dst: Path, shift: int) -> list[int]:
    data = bytearray(src.read_bytes())
    start, end = find_pdta(data)
    off, size = find_shdr(data, start, end)
    roots: list[int] = []
    pos = 0
    while pos + SHDR_RECORD <= size:
        rec = off + pos
        name = bytes(data[rec : rec + 20]).split(b"\0", 1)[0]
        if not name:  # terminal record
            break
        root = data[rec + ROOT_OFFSET]
        if root:
            new = root + shift
            if not 0 <= new <= 127:
                raise ValueError(f"sample {name!r} root {root} + {shift} is outside 0..127")
            data[rec + ROOT_OFFSET] = new
        roots.append(root)
        pos += SHDR_RECORD
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_bytes(data)
    return roots


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("input", type=Path)
    ap.add_argument("output", type=Path)
    ap.add_argument(
        "--root-shift",
        type=int,
        required=True,
        help="semitones added to every sample root (-12 renders one octave higher)",
    )
    args = ap.parse_args()
    roots = rekey(args.input, args.output, args.root_shift)
    print(f"{args.input} -> {args.output}")
    print(f"shifted {len(roots)} sample roots by {args.root_shift:+d}: {roots}")


if __name__ == "__main__":
    main()
