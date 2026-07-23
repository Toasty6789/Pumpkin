import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import chunk_parity


class RegionAddressingTests(unittest.TestCase):
    def test_region_slot_handles_negative_chunk_coordinates(self):
        self.assertEqual(chunk_parity.region_location(0, 0), (0, 0, 0))
        self.assertEqual(chunk_parity.region_location(31, 31), (0, 0, 1023))
        self.assertEqual(chunk_parity.region_location(32, 0), (1, 0, 0))
        self.assertEqual(chunk_parity.region_location(-1, -1), (-1, -1, 1023))
        self.assertEqual(chunk_parity.region_location(-32, -32), (-1, -1, 0))
        self.assertEqual(chunk_parity.region_location(-33, 0), (-2, 0, 31))


class BlockStateNormalizationTests(unittest.TestCase):
    def test_numeric_and_named_palettes_preserve_properties(self):
        named = {"Name": "minecraft:oak_log", "Properties": {"axis": "x"}}
        self.assertEqual(
            chunk_parity.palette_name(named, chunk_parity.STATE_NAMES),
            chunk_parity.STATE_NAMES[136],
        )
        self.assertEqual(chunk_parity.STATE_NAMES[136], "minecraft:oak_log[axis=x]")

    def test_boolean_state_order_matches_minecraft_registry(self):
        self.assertEqual(chunk_parity.STATE_NAMES[8], "minecraft:grass_block[snowy=true]")
        self.assertEqual(chunk_parity.STATE_NAMES[9], "minecraft:grass_block[snowy=false]")


class StreamingFieldComparisonTests(unittest.TestCase):
    def test_block_state_properties_participate_in_chunk_equality(self):
        def root(axis):
            return {
                "sections": [
                    {
                        "Y": 0,
                        "block_states": {
                            "palette": [
                                {"Name": "minecraft:oak_log", "Properties": {"axis": axis}}
                            ]
                        },
                        "biomes": {"palette": ["minecraft:plains"]},
                    }
                ],
                "Heightmaps": {},
                "structures": {},
            }

        self.assertTrue(all(chunk_parity.compare_chunk_fields(root("x"), root("x")).values()))
        fields = chunk_parity.compare_chunk_fields(root("x"), root("z"))
        self.assertFalse(fields["blocks"])
        self.assertTrue(fields["biomes"])


class ComparableChunkTests(unittest.TestCase):
    def test_only_full_chunks_are_comparable(self):
        self.assertTrue(chunk_parity.is_full_chunk({"Status": "minecraft:full"}))
        self.assertFalse(chunk_parity.is_full_chunk({"Status": "minecraft:surface"}))
        self.assertFalse(chunk_parity.is_full_chunk({}))


class RadiusMaskTests(unittest.TestCase):
    def test_circle_uses_chunk_aabb_intersection(self):
        self.assertTrue(chunk_parity.chunk_intersects_circle(62, 0, 1000))
        self.assertFalse(chunk_parity.chunk_intersects_circle(63, 0, 1000))
        self.assertTrue(chunk_parity.chunk_intersects_circle(-63, 0, 1000))
        self.assertFalse(chunk_parity.chunk_intersects_circle(-64, 0, 1000))

    def test_one_thousand_block_circle_contains_expected_chunks(self):
        chunks = chunk_parity.chunk_circle(1000)
        self.assertEqual(len(chunks), 12522)
        self.assertEqual(len(chunks), len(set(chunks)))


if __name__ == "__main__":
    unittest.main()
