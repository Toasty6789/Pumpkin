#!/usr/bin/env python3
"""Block/biome/heightmap/structure differential for modern Anvil worlds.

Usage:
  uv run --with nbtlib --with lz4 scripts/chunk_parity.py VANILLA_WORLD PUMPKIN_WORLD

The world arguments may point either at a world root or directly at a region
folder. Every chunk present in both inputs is decoded using the modern 26.2
paletted-container format and compared by base block name, biome, heightmap,
and structure NBT. Pumpkin's compact Anvil palette stores numeric state IDs,
so state properties are deliberately excluded from the cross-format block hash.
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
import math
import re
import struct
import zlib
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Any, Iterable

import nbtlib


def load_numeric_palettes() -> tuple[dict[int, str], dict[int, str]]:
    asset_root = Path(__file__).resolve().parents[1] / "assets"
    blocks = json.loads((asset_root / "blocks.json").read_text(encoding="utf-8"))
    state_names = {
        int(state["id"]): f"minecraft:{block['name']}"
        for block in blocks["blocks"]
        for state in block["states"]
    }
    biomes = json.loads((asset_root / "biome.json").read_text(encoding="utf-8"))
    biome_names = {
        int(data["id"]): f"minecraft:{name}" for name, data in biomes.items()
    }
    return state_names, biome_names


STATE_NAMES, BIOME_NAMES = load_numeric_palettes()


def decompress_lz4_block_stream(data: bytes) -> bytes:
    """Decode the LZ4Block stream used by modern Java Anvil compression id 4."""
    import lz4.block

    output = bytearray()
    offset = 0
    magic = b"LZ4Block"
    while offset < len(data):
        if data[offset : offset + len(magic)] != magic:
            raise ValueError(f"Invalid LZ4Block magic at offset {offset}")
        offset += len(magic)
        if offset + 13 > len(data):
            raise ValueError("Truncated LZ4Block header")
        token = data[offset]
        offset += 1
        compressed_len, decompressed_len, _checksum = struct.unpack_from(
            "<III", data, offset
        )
        offset += 12
        if compressed_len == 0 and decompressed_len == 0:
            break
        block = data[offset : offset + compressed_len]
        if len(block) != compressed_len:
            raise ValueError("Truncated LZ4Block payload")
        offset += compressed_len
        method = token & 0xF0
        if method == 0x10:
            decoded = block
        elif method == 0x20:
            decoded = lz4.block.decompress(block, uncompressed_size=decompressed_len)
        else:
            raise ValueError(f"Unsupported LZ4Block method {method:#x}")
        if len(decoded) != decompressed_len:
            raise ValueError("LZ4Block decompressed length mismatch")
        output.extend(decoded)
    return bytes(output)

REGION_RE = re.compile(r"r\.(-?\d+)\.(-?\d+)\.mca$")


def find_region_dir(world: Path) -> Path:
    candidates = [
        world,
        world / "region",
        world / "dimensions" / "minecraft" / "overworld" / "region",
    ]
    for candidate in candidates:
        if candidate.is_dir() and any(candidate.glob("r.*.*.mca")):
            return candidate
    raise FileNotFoundError(f"No Anvil region directory below {world}")


def normalized(value: Any) -> Any:
    if hasattr(value, "unpack"):
        value = value.unpack(json=True)
    if isinstance(value, dict):
        return {str(k): normalized(v) for k, v in sorted(value.items(), key=lambda p: str(p[0]))}
    if isinstance(value, (list, tuple)):
        return [normalized(v) for v in value]
    return value


def read_region_chunks(path: Path) -> Iterable[tuple[tuple[int, int], Any]]:
    match = REGION_RE.match(path.name)
    if not match:
        return
    region_x, region_z = map(int, match.groups())
    with path.open("rb") as stream:
        locations = stream.read(4096)
        for local_z in range(32):
            for local_x in range(32):
                index = local_x + local_z * 32
                location = struct.unpack_from(">I", locations, index * 4)[0]
                sector_offset = location >> 8
                if sector_offset == 0:
                    continue
                stream.seek(sector_offset * 4096)
                length_raw = stream.read(4)
                if len(length_raw) != 4:
                    continue
                length = struct.unpack(">I", length_raw)[0]
                compression = stream.read(1)[0]
                payload = stream.read(length - 1)
                if compression == 1:
                    payload = gzip.decompress(payload)
                elif compression == 2:
                    payload = zlib.decompress(payload)
                elif compression == 3:
                    pass
                elif compression == 4:
                    payload = decompress_lz4_block_stream(payload)
                else:
                    raise ValueError(f"Unsupported compression {compression} in {path}")
                root = nbtlib.File.parse(io.BytesIO(payload))
                x = int(root.get("xPos", region_x * 32 + local_x))
                z = int(root.get("zPos", region_z * 32 + local_z))
                yield (x, z), root


def palette_name(entry: Any, numeric_names: dict[int, str]) -> str:
    data = normalized(entry)
    if isinstance(data, str):
        return data
    if isinstance(data, int):
        return numeric_names.get(data, f"<unknown-id:{data}>")
    name = data.get("Name", data.get("name", "<unknown>"))
    return str(name)


def unpack_palette_indices(raw_longs: Iterable[int], palette_size: int, count: int, minimum_bits: int) -> list[int]:
    if palette_size <= 1:
        return [0] * count
    bits = max(minimum_bits, math.ceil(math.log2(palette_size)))
    values_per_long = 64 // bits
    mask = (1 << bits) - 1
    longs = [int(value) & 0xFFFF_FFFF_FFFF_FFFF for value in raw_longs]
    result: list[int] = []
    for word in longs:
        for slot in range(values_per_long):
            result.append((word >> (slot * bits)) & mask)
            if len(result) == count:
                return result
    if len(result) < count:
        raise ValueError(f"Paletted container truncated: got {len(result)} of {count} values")
    return result


def decode_container(
    container: Any,
    count: int,
    minimum_bits: int,
    numeric_names: dict[int, str],
) -> tuple[str, ...]:
    if container is None:
        return tuple()
    palette = container.get("palette", container.get("Palette", []))
    names = [palette_name(item, numeric_names) for item in palette]
    if not names:
        return tuple()
    data = container.get("data", container.get("BlockStates", []))
    indices = unpack_palette_indices(data, len(names), count, minimum_bits)
    return tuple(names[index] if index < len(names) else "<invalid-palette-index>" for index in indices)


def chunk_fingerprint(root: Any) -> dict[str, Any]:
    sections = root.get("sections", root.get("Sections", []))
    blocks: dict[int, tuple[str, ...]] = {}
    biomes: dict[int, tuple[str, ...]] = {}
    for section in sections:
        y = int(section.get("Y", 0))
        block_states = section.get("block_states")
        if block_states is not None:
            blocks[y] = decode_container(block_states, 4096, 4, STATE_NAMES)
        biome_states = section.get("biomes")
        if biome_states is not None:
            biomes[y] = decode_container(biome_states, 64, 1, BIOME_NAMES)
    return {
        "blocks": blocks,
        "biomes": biomes,
        "heightmaps": normalized(root.get("Heightmaps", {})),
        "structures": normalized(root.get("structures", root.get("Structures", {}))),
        "status": str(root.get("Status", "")),
    }


def stable_hash(value: Any) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":"), default=str).encode()
    return hashlib.sha256(encoded).hexdigest()


@dataclass
class Report:
    vanilla_region: str
    pumpkin_region: str
    vanilla_chunks: int
    pumpkin_chunks: int
    overlapping_chunks: int
    exact_chunks: int
    block_exact_chunks: int
    biome_exact_chunks: int
    heightmap_exact_chunks: int
    structure_exact_chunks: int
    missing_from_pumpkin: list[list[int]]
    extra_in_pumpkin: list[list[int]]
    mismatches: list[dict[str, Any]]


def load_world(region_dir: Path) -> dict[tuple[int, int], dict[str, Any]]:
    chunks: dict[tuple[int, int], dict[str, Any]] = {}
    for region in sorted(region_dir.glob("r.*.*.mca")):
        for position, root in read_region_chunks(region):
            chunks[position] = chunk_fingerprint(root)
    return chunks


def compare(vanilla_dir: Path, pumpkin_dir: Path, mismatch_limit: int) -> Report:
    vanilla = load_world(vanilla_dir)
    pumpkin = load_world(pumpkin_dir)
    overlap = sorted(vanilla.keys() & pumpkin.keys())
    exact = block_exact = biome_exact = height_exact = structure_exact = 0
    mismatches: list[dict[str, Any]] = []
    for position in overlap:
        left, right = vanilla[position], pumpkin[position]
        fields = {
            "blocks": stable_hash(left["blocks"]) == stable_hash(right["blocks"]),
            "biomes": stable_hash(left["biomes"]) == stable_hash(right["biomes"]),
            "heightmaps": stable_hash(left["heightmaps"]) == stable_hash(right["heightmaps"]),
            "structures": stable_hash(left["structures"]) == stable_hash(right["structures"]),
        }
        block_exact += fields["blocks"]
        biome_exact += fields["biomes"]
        height_exact += fields["heightmaps"]
        structure_exact += fields["structures"]
        if all(fields.values()):
            exact += 1
        elif len(mismatches) < mismatch_limit:
            mismatches.append({"chunk": list(position), "equal": fields})
    return Report(
        vanilla_region=str(vanilla_dir),
        pumpkin_region=str(pumpkin_dir),
        vanilla_chunks=len(vanilla),
        pumpkin_chunks=len(pumpkin),
        overlapping_chunks=len(overlap),
        exact_chunks=exact,
        block_exact_chunks=block_exact,
        biome_exact_chunks=biome_exact,
        heightmap_exact_chunks=height_exact,
        structure_exact_chunks=structure_exact,
        missing_from_pumpkin=[list(p) for p in sorted(vanilla.keys() - pumpkin.keys())],
        extra_in_pumpkin=[list(p) for p in sorted(pumpkin.keys() - vanilla.keys())],
        mismatches=mismatches,
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("vanilla_world", type=Path)
    parser.add_argument("pumpkin_world", type=Path)
    parser.add_argument("--json", type=Path)
    parser.add_argument("--mismatch-limit", type=int, default=100)
    args = parser.parse_args()
    report = compare(
        find_region_dir(args.vanilla_world),
        find_region_dir(args.pumpkin_world),
        args.mismatch_limit,
    )
    payload = json.dumps(asdict(report), indent=2)
    print(payload)
    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(payload + "\n", encoding="utf-8")
    return 0 if report.overlapping_chunks and report.exact_chunks == report.overlapping_chunks else 1


if __name__ == "__main__":
    raise SystemExit(main())
