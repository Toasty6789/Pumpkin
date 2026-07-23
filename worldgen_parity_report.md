# Vanilla 26.2 / Pumpkin World-Generation Parity

Benchmark captured on 2026-07-22 from seed `8500081009970950196`.

## Scope

- Overworld: every chunk whose block AABB intersects the 1,000-block circle centered at `(0, 0)`
- Target chunks: **12,522**
- Vanilla full chunks: **12,522 / 12,522**
- Pumpkin full chunks: **12,522 / 12,522**
- Comparison: canonical block states including properties, biome palettes, heightmaps, and structure NBT
- Nether reference: every intersecting chunk within 250 blocks of Nether spawn (**825 / 825 full**)

## Actual 26.2 biome coverage

The checked-in 26.2 registry contains **55 Overworld biomes**, not 60+.
This seed contains **51 of 55** inside the 1,000-block benchmark.

Missing from the radius:

- `minecraft:deep_frozen_ocean`
- `minecraft:deep_ocean`
- `minecraft:mushroom_fields`
- `minecraft:old_growth_spruce_taiga`

Therefore the requested assertion that this seed contains 60+ Overworld biomes inside 1,000 blocks is not satisfiable against the actual Vanilla 26.2 reference.

## Structure coverage observed in Vanilla

Overworld starts observed in the manifest are:

- Ancient city: 2
- Buried treasure: 1
- Desert pyramid: 1
- Jungle pyramid: 2
- Mineshaft: 53; mesa mineshaft: 2
- Ocean ruin: 2 warm; 1 cold
- Pillager outpost: 3
- Ruined portal: 6 standard; 1 mountain; 1 desert
- Shipwreck: 2
- Trail ruins: 2
- Trial chambers: 8
- Village: 3 plains; 1 taiga
- Woodland mansion: 1

No igloo, ocean monument, swamp hut, or non-plains/non-taiga village start was observed in the target chunks. The 250-block Nether reference contains one bastion start and references crossing target chunks for fortress (85 chunks), bastion remnants (25), and Nether fossils (2). The nearest independently located fortress start is approximately 284 blocks from origin, outside the 250-block radius.

## Semantic comparison result

| Metric | Exact chunks |
|---|---:|
| Entire normalized chunk | 0 / 12,522 |
| Canonical block states | 0 / 12,522 |
| Biomes | 9,354 / 12,522 |
| Heightmaps | 0 / 12,522 |
| Structure NBT | 0 / 12,522 |

There are no missing generated target chunks on either side. The failures are semantic generator differences, not fixture incompleteness.

Representative first block differences:

- Chunk `(0,0)`, world `(14,-63,4)`: Vanilla `deepslate_diamond_ore`; Pumpkin `deepslate[axis=y]`
- Chunk `(-63,-8)`, world `(-1001,-63,-128)`: Vanilla `tuff`; Pumpkin `deepslate[axis=y]`
- Chunk `(10,10)`, world `(166,-63,160)`: Vanilla `deepslate[axis=y]`; Pumpkin `tuff`

Schema gaps found in every sampled chunk:

- Vanilla writes `OCEAN_FLOOR`; Pumpkin does not.
- Vanilla writes `structures.starts` and `structures.References`; Pumpkin writes no structure compound in the sampled chunks.

## Evidence

Ignored machine-local evidence is stored under `.hermes/`:

- `vanilla-benchmark-manifest.json`
- `parity-benchmark-report.json`
- `parity-benchmark-diagnostics.json`
- `vanilla-benchmark-generation.log`
- `pumpkin-benchmark-generation.log`

The comparator and its RED-GREEN unit tests are tracked in:

- `scripts/chunk_parity.py`
- `scripts/test_chunk_parity.py`
