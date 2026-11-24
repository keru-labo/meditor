import io
import stat
import tempfile
import unittest
import warnings
from pathlib import Path
from unittest.mock import patch
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo

import gherkin_report_catalog as catalog

FEATURE = '''@feature
Feature: Export
  Background:
    Given the editor is open
  @rule
  Rule: Apply the selected format
    Background:
      Given a project with captions
    @planned
    Scenario Outline: Export <output mode[1]>
      When I select <output mode[1]>
      And start export
      Then the parameters are written
        | format | <output mode[1]> |
      And the metadata is written
        """text/<output mode[1]>
        mode=<output mode[1]>
        """
      @examples
      Examples:
        | output mode[1] |
        | mp4            |
        | mov            |
'''


def make_archive(*entries):
    buffer = io.BytesIO()
    with ZipFile(buffer, "w", compression=ZIP_DEFLATED) as archive:
        for path, content in entries:
            archive.writestr(path, content)
    buffer.seek(0)
    return ZipFile(buffer)


class CatalogTests(unittest.TestCase):
    def test_expands_gherkin_with_inheritance_and_literal_examples(self):
        with make_archive(("docs/features/more/export.feature", FEATURE)) as archive:
            [feature] = catalog.read_catalog(archive)
            self.assertEqual(catalog.read_catalog(archive), [feature])

        self.assertEqual(feature["name"], "Export")
        self.assertEqual(feature["uri"], "docs/features/more/export.feature")
        mp4, mov = feature["scenarios"]
        self.assertEqual([mp4["name"], mov["name"]], ["Export mp4", "Export mov"])
        self.assertNotEqual(mp4["id"], mov["id"])
        self.assertLess(mp4["line"], mov["line"])
        self.assertEqual(mp4["rule"], "Apply the selected format")
        self.assertTrue(mp4["planned"])
        self.assertEqual(mp4["tags"], ["@feature", "@rule", "@planned", "@examples"])
        self.assertEqual(mp4["example"], {"line": mp4["line"], "values": [("output mode[1]", "mp4")]})
        self.assertEqual(mov["steps"][2]["text"], "I select mov")
        self.assertEqual(
            [step["kind"] for step in mp4["steps"]],
            ["Context", "Context", "Action", "Action", "Outcome", "Outcome"],
        )
        self.assertEqual([step["background"] for step in mp4["steps"]], [True, True, False, False, False, False])
        self.assertEqual(mp4["steps"][4]["argument"], {"kind": "table", "rows": [["format", "mp4"]]})
        self.assertEqual(
            mp4["steps"][5]["argument"],
            {"kind": "doc_string", "content": "mode=mp4", "media_type": "text/mp4"},
        )

    def test_separate_examples_blocks_create_rows_and_keep_their_own_tags(self):
        more = FEATURE + '''      @second
      Examples:
        | output mode[1] |
        | webm           |
'''
        with make_archive(("docs/features/export.feature", more)) as archive:
            cases = catalog.read_catalog(archive)[0]["scenarios"]
        self.assertEqual([case["name"] for case in cases], ["Export mp4", "Export mov", "Export webm"])
        self.assertIn("@second", cases[-1]["tags"])
        self.assertNotIn("@examples", cases[-1]["tags"])

    def test_examples_can_include_angle_brackets_and_overlapping_names(self):
        source = '''Feature: Export
  Scenario Outline: Export <file>limit>
    Given <file>limit> clips
    When I select <source<format> for <file>
    Then <unknown> stays literal
    Examples:
      | file  | file>limit | source<format |
      | clips | 10         | mp4           |
'''
        with make_archive(("docs/features/export.feature", source)) as archive:
            [case] = catalog.read_catalog(archive)[0]["scenarios"]
        self.assertEqual(case["name"], "Export 10")
        self.assertEqual([step["text"] for step in case["steps"]], [
            "10 clips", "I select mp4 for clips", "<unknown> stays literal",
        ])

    def test_binds_only_the_two_tagged_ui_cases_in_the_repo(self):
        with tempfile.TemporaryDirectory() as temporary:
            staging = Path(temporary)
            catalog.collect_feature_sources(staging)
            files = sorted(staging.rglob("*.feature"))
            with make_archive(*[(file.relative_to(staging).as_posix(), file.read_bytes()) for file in files]) as archive:
                features = catalog.read_catalog(archive)

        self.assertEqual({feature["uri"] for feature in features},
                         {file.relative_to(staging).as_posix() for file in files})
        bound = [(feature["uri"], scenario["evidence_id"])
                 for feature in features for scenario in feature["scenarios"] if scenario["evidence_id"]]
        self.assertCountEqual(bound, [
            (catalog.HISTORY_FEATURE, "ui-redo-after-edit"),
            (catalog.HISTORY_FEATURE, "ui-reopen-history"),
        ])

    def test_rejects_unsafe_zip_paths_symlinks_and_duplicate_entries(self):
        link = ZipInfo("docs/features/link.feature")
        link.create_system = 3
        link.external_attr = (stat.S_IFLNK | 0o777) << 16
        path = "docs/features/export.feature"
        for entries in [
            (("docs/features/../export.feature", FEATURE),),
            (("/docs/features/export.feature", FEATURE),),
            ((link, "elsewhere.feature"),),
            ((path, FEATURE), (path, FEATURE)),
        ]:
            with self.subTest(entries=[name for name, _ in entries]), warnings.catch_warnings():
                warnings.filterwarnings("ignore", message="Duplicate name")
                with make_archive(*entries) as archive, self.assertRaises(ValueError):
                    catalog.read_catalog(archive)

    def test_limits_number_and_size_of_sources_and_expanded_cases(self):
        path = "docs/features/export.feature"
        for limit, value, message in [
            ("MAX_ARCHIVE_ENTRIES", 0, "archive entries"),
            ("MAX_FILES", 0, "number of Gherkin files"),
            ("MAX_FILE_BYTES", 64, "source is too large"),
            ("MAX_SOURCE_BYTES", 64, "sources are too large"),
            ("MAX_CASES", 1, "expansion exceeds"),
            ("MAX_STEPS", 11, "expansion exceeds"),
            ("MAX_TAGS", 7, "expansion exceeds"),
            ("MAX_MATCH_WORK", 0, "Examples matching exceeds"),
            ("MAX_EXPANDED_BYTES", 550, "arguments are too large"),
            ("MAX_EXPANDED_BYTES", 2000, "catalog is too large"),
        ]:
            with self.subTest(limit=limit, value=value), patch.object(catalog, limit, value):
                with make_archive((path, FEATURE)) as archive, self.assertRaisesRegex(ValueError, message):
                    catalog.read_catalog(archive)

    def test_rejects_oversize_interpolated_argument_before_allocating_it(self):
        source = FEATURE.replace("mode=<output mode[1]>", "<output mode[1]>" * 3)
        source = source.replace("| mp4            |", f"| {'x' * 3000} |")
        with make_archive(("docs/features/export.feature", source)) as archive:
            with self.assertRaisesRegex(ValueError, "Expanded Gherkin text is too long"):
                catalog.read_catalog(archive)

    def test_small_compressed_source_cannot_multiply_background_arguments(self):
        body = "x" * 7000
        source = (
            'Feature: Review\n  Background:\n    Given a first fixture\n      """\n'
            f'      {body}\n      """\n    And a second fixture\n      """\n'
            f'      {body}\n      """\n  Scenario Outline: Verify <n>\n'
            '    Then it works\n    Examples:\n      | n |\n'
            + ''.join(f'      | {row} |\n' for row in range(325))
        )
        with make_archive(("docs/features/large.feature", source)) as archive:
            with self.assertRaisesRegex(ValueError, "Expanded Gherkin arguments are too large"):
                catalog.read_catalog(archive)

    def test_rejects_evidence_tag_outside_its_ui_feature(self):
        with make_archive(("docs/features/export.feature", FEATURE.replace("@planned", "@ui-redo-after-edit"))) as archive:
            with self.assertRaisesRegex(ValueError, "invalid UI evidence tag"):
                catalog.read_catalog(archive)

    def test_rejects_examples_without_runnable_rows(self):
        source = FEATURE.replace("        | mp4            |\n", "").replace("        | mov            |\n", "")
        with make_archive(("docs/features/export.feature", source)) as archive:
            with self.assertRaisesRegex(ValueError, "no runnable Examples row"):
                catalog.read_catalog(archive)


if __name__ == "__main__":
    unittest.main()
