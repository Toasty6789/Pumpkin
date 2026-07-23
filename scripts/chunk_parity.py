#!/usr/bin/env python3
"""Block/biome/heightmap/structure differential for modern Anvil worlds.

Usage:
  uv run --with nbtlib --with lz4 scripts/chunk_parity.py VANILLA_WORLD PUMPKIN_WORLD

The world arguments may point either at a world root or directly at a region
folder. Only chunks whose status is `minecraft:full` are compared. Every selected
chunk is decoded using the modern 26.2 paletted-container format and compared by
canonical block state (name plus properties), biome, heightmap, and structure NBT.
Pumpkin's compact Anvil palette stores numeric block-state and biome IDs; those IDs
are normalized through the generated 26.2 assets before comparison.
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import itertools
import json
import math
import re
import struct
import zlib
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Any, Iterable

import nbtlib


def canonical_block_state(name: str, properties: dict[str, Any] | None = None) -> str:
    if not properties:
        return name
    serialized = ",".join(
        f"{key}={str(value).lower()}" for key, value in sorted(properties.items())
    )
    return f"{name}[{serialized}]"


def load_numeric_palettes() -> tuple[dict[int, str], dict[int, str]]:
    asset_root = Path(__file__).resolve().parents[1] / "assets"
    blocks = json.loads((asset_root / "blocks.json").read_text(encoding="utf-8"))
    property_assets = json.loads(
        (asset_root / "properties.json").read_text(encoding="utf-8")
    )
    properties_by_hash = {
        int(property_data["hash_key"]): property_data
        for property_data in property_assets
    }
    state_names: dict[int, str] = {}
    for block in blocks["blocks"]:
        definitions = [properties_by_hash[int(key)] for key in block["properties"]]
        value_sets: list[list[str]] = []
        for definition in definitions:
            property_type = definition["type"]
            if property_type == "boolean":
                # Minecraft's BooleanProperty iterates true before false.
                values = ["true", "false"]
            elif property_type == "int":
                values = [
                    str(value)
                    for value in range(int(definition["min"]), int(definition["max"]) + 1)
                ]
            else:
                values = [str(value) for value in definition["values"]]
            value_sets.append(values)
        combinations = list(itertools.product(*value_sets)) if value_sets else [tuple()]
        if len(combinations) != len(block["states"]):
            raise ValueError(
                f"State/property cardinality mismatch for {block['name']}: "
                f"{len(block['states'])} states versus {len(combinations)} combinations"
            )
        for state, values in zip(block["states"], combinations, strict=True):
            properties = {
                definition["serialized_name"]: value
                for definition, value in zip(definitions, values, strict=True)
            }
            state_names[int(state["id"])] = canonical_block_state(
                f"minecraft:{block['name']}", properties
            )
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


def is_full_chunk(root: Any) -> bool:
    """Return whether a chunk reached the only status valid for block parity."""
    return str(root.get("Status", "")) == "minecraft:full"


def chunk_intersects_circle(chunk_x: int, chunk_z: int, radius_blocks: int) -> bool:
    """Test a chunk's inclusive block AABB against a circle centered on zero."""
    if radius_blocks < 0:
        raise ValueError("radius_blocks must be non-negative")
    block_min_x, block_max_x = chunk_x * 16, chunk_x * 16 + 15
    block_min_z, block_max_z = chunk_z * 16, chunk_z * 16 + 15
    nearest_x = 0 if block_min_x <= 0 <= block_max_x else min(abs(block_min_x), abs(block_max_x))
    nearest_z = 0 if block_min_z <= 0 <= block_max_z else min(abs(block_min_z), abs(block_max_z))
    return nearest_x * nearest_x + nearest_z * nearest_z <= radius_blocks * radius_blocks


def chunk_circle(radius_blocks: int) -> list[tuple[int, int]]:
    """Enumerate every chunk whose block AABB intersects the requested circle."""
    if radius_blocks < 0:
        raise ValueError("radius_blocks must be non-negative")
    minimum = math.floor(-radius_blocks / 16) - 1
    maximum = math.floor(radius_blocks / 16) + 1
    return [
        (chunk_x, chunk_z)
        for chunk_z in range(minimum, maximum + 1)
        for chunk_x in range(minimum, maximum + 1)
        if chunk_intersects_circle(chunk_x, chunk_z, radius_blocks)
    ]


def region_location(chunk_x: int, chunk_z: int) -> tuple[int, int, int]:
    """Return region coordinates and location-table slot for a chunk."""
    region_x, region_z = chunk_x // 32, chunk_z // 32
    local_x, local_z = chunk_x % 32, chunk_z % 32
    return region_x, region_z, local_x + local_z * 32


def decode_chunk_payload(payload: bytes, compression: int, path: Path) -> Any:
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
    return nbtlib.File.parse(io.BytesIO(payload))


def read_region_chunk(region_dir: Path, chunk_x: int, chunk_z: int) -> Any | None:
    """Read one chunk without materializing every chunk in a benchmark world."""
    region_x, region_z, index = region_location(chunk_x, chunk_z)
    path = region_dir / f"r.{region_x}.{region_z}.mca"
    if not path.is_file():
        return None
    with path.open("rb") as stream:
        stream.seek(index * 4)
        location_raw = stream.read(4)
        if len(location_raw) != 4:
            return None
        location = struct.unpack(">I", location_raw)[0]
        sector_offset = location >> 8
        if sector_offset == 0:
            return None
        stream.seek(sector_offset * 4096)
        length_raw = stream.read(4)
        if len(length_raw) != 4:
            return None
        length = struct.unpack(">I", length_raw)[0]
        compression_raw = stream.read(1)
        if not compression_raw or length < 1:
            return None
        payload = stream.read(length - 1)
        if len(payload) != length - 1:
            return None
        return decode_chunk_payload(payload, compression_raw[0], path)


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
                root = decode_chunk_payload(payload, compression, path)
                x = int(root.get("xPos", region_x * 32 + local_x))
                z = int(root.get("zPos", region_z * 32 + local_z))
                yield (x, z), root


def palette_name(entry: Any, numeric_names: dict[int, str]) -> str:
    data = normalized(entry)
    if isinstance(data, str):
        return data
    if isinstance(data, int):
        return numeric_names.get(data, f"<unknown-id:{data}>")
    name = str(data.get("Name", data.get("name", "<unknown>")))
    properties = data.get("Properties", data.get("properties", {}))
    return canonical_block_state(name, properties)


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


def compare_chunk_fields(left_root: Any, right_root: Any) -> dict[str, bool]:
    """Compare one chunk semantically while short-circuiting mismatched sections."""
    left_sections = {int(section["Y"]): section for section in left_root.get("sections", [])}
    right_sections = {int(section["Y"]): section for section in right_root.get("sections", [])}
    section_levels = sorted(left_sections.keys() | right_sections.keys())

    blocks_equal = True
    for level in section_levels:
        left_container = left_sections.get(level, {}).get("block_states")
        right_container = right_sections.get(level, {}).get("block_states")
        left_blocks = (
            decode_container(left_container, 4096, 4, STATE_NAMES)
            if left_container is not None
            else ("minecraft:air",) * 4096
        )
        right_blocks = (
            decode_container(right_container, 4096, 4, STATE_NAMES)
            if right_container is not None
            else ("minecraft:air",) * 4096
        )
        if left_blocks != right_blocks:
            blocks_equal = False
            break

    biomes_equal = True
    for level in section_levels:
        left_container = left_sections.get(level, {}).get("biomes")
        right_container = right_sections.get(level, {}).get("biomes")
        if left_container is None or right_container is None:
            if left_container is right_container:
                continue
            biomes_equal = False
            break
        if decode_container(left_container, 64, 1, BIOME_NAMES) != decode_container(
            right_container, 64, 1, BIOME_NAMES
        ):
            biomes_equal = False
            break

    return {
        "blocks": blocks_equal,
        "biomes": biomes_equal,
        "heightmaps": normalized(left_root.get("Heightmaps", {}))
        == normalized(right_root.get("Heightmaps", {})),
        "structures": normalized(
            left_root.get("structures", left_root.get("Structures", {}))
        )
        == normalized(right_root.get("structures", right_root.get("Structures", {}))),
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


def load_world(
    region_dir: Path,
    chunk_mask: set[tuple[int, int]] | None = None,
) -> dict[tuple[int, int], dict[str, Any]]:
    chunks: dict[tuple[int, int], dict[str, Any]] = {}
    for region in sorted(region_dir.glob("r.*.*.mca")):
        for position, root in read_region_chunks(region):
            if chunk_mask is not None and position not in chunk_mask:
                continue
            if not is_full_chunk(root):
                continue
            chunks[position] = chunk_fingerprint(root)
    return chunks


def compare_masked_streaming(
    vanilla_dir: Path,
    pumpkin_dir: Path,
    mismatch_limit: int,
    chunk_mask: set[tuple[int, int]],
) -> Report:
    """Compare a large target set one chunk at a time to keep memory bounded."""
    vanilla_positions: set[tuple[int, int]] = set()
    pumpkin_positions: set[tuple[int, int]] = set()
    exact = block_exact = biome_exact = height_exact = structure_exact = 0
    mismatches: list[dict[str, Any]] = []

    for position in sorted(chunk_mask):
        left_root = read_region_chunk(vanilla_dir, *position)
        right_root = read_region_chunk(pumpkin_dir, *position)
        left_full = left_root is not None and is_full_chunk(left_root)
        right_full = right_root is not None and is_full_chunk(right_root)
        if left_full:
            vanilla_positions.add(position)
        if right_full:
            pumpkin_positions.add(position)
        if not (left_full and right_full):
            continue

        fields = compare_chunk_fields(left_root, right_root)
        block_exact += fields["blocks"]
        biome_exact += fields["biomes"]
        height_exact += fields["heightmaps"]
        structure_exact += fields["structures"]
        if all(fields.values()):
            exact += 1
        elif len(mismatches) < mismatch_limit:
            mismatches.append({"chunk": list(position), "equal": fields})

    overlap = vanilla_positions & pumpkin_positions
    return Report(
        vanilla_region=str(vanilla_dir),
        pumpkin_region=str(pumpkin_dir),
        vanilla_chunks=len(vanilla_positions),
        pumpkin_chunks=len(pumpkin_positions),
        overlapping_chunks=len(overlap),
        exact_chunks=exact,
        block_exact_chunks=block_exact,
        biome_exact_chunks=biome_exact,
        heightmap_exact_chunks=height_exact,
        structure_exact_chunks=structure_exact,
        missing_from_pumpkin=[
            list(position) for position in sorted(vanilla_positions - pumpkin_positions)
        ],
        extra_in_pumpkin=[
            list(position) for position in sorted(pumpkin_positions - vanilla_positions)
        ],
        mismatches=mismatches,
    )


def compare(
    vanilla_dir: Path,
    pumpkin_dir: Path,
    mismatch_limit: int,
    chunk_mask: set[tuple[int, int]] | None = None,
) -> Report:
    if chunk_mask is not None:
        return compare_masked_streaming(
            vanilla_dir, pumpkin_dir, mismatch_limit, chunk_mask
        )
    vanilla = load_world(vanilla_dir, chunk_mask)
    pumpkin = load_world(pumpkin_dir, chunk_mask)
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
    parser.add_argument(
        "--radius-blocks",
        type=int,
        help="compare only full chunks intersecting this radius around block (0,0)",
    )
    args = parser.parse_args()
    chunk_mask = set(chunk_circle(args.radius_blocks)) if args.radius_blocks is not None else None
    report = compare(
        find_region_dir(args.vanilla_world),
        find_region_dir(args.pumpkin_world),
        args.mismatch_limit,
        chunk_mask,
    )
    payload = json.dumps(asdict(report), indent=2)
    print(payload)
    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(payload + "\n", encoding="utf-8")
    return 0 if report.overlapping_chunks and report.exact_chunks == report.overlapping_chunks else 1


if __name__ == "__main__":
    raise SystemExit(main())
