import io
import json
import runpy
import struct
import tempfile
import unittest
import zipfile
from pathlib import Path

from gherkin_report_catalog import read_catalog

report = runpy.run_path(str(Path(__file__).with_name("render-history-report.py")))


class HistoryReportTest(unittest.TestCase):
    def setUp(self):
        docs = Path(__file__).resolve().parent.parent / "docs/features"
        sources = io.BytesIO()
        with zipfile.ZipFile(sources, "w") as archive:
            for path in docs.rglob("*.feature"):
                archive.write(path, f"docs/features/{path.relative_to(docs).as_posix()}")
        with zipfile.ZipFile(sources) as archive:
            self.catalogue = read_catalog(archive)
        self.cases = [case for feature in self.catalogue for case in feature["scenarios"]]
        self.captured = {case["evidence_id"]: case for case in self.cases if case["evidence_id"]}

    def evidence_zip(self, first_failure=False, swap_tags=False, wrong_spec=False, swap_invalid_tags=False):
        records = []
        payload = io.BytesIO()
        png = report["PNG_HEADER"] + struct.pack(">II", 1, 1)
        counts = report["CAPTURE_STAGE_COUNTS"]
        with zipfile.ZipFile(payload, "w") as archive:
            for index, (tag, count) in enumerate(counts.items(), start=1):
                case = self.captured[tag]
                stages = []
                failed = first_failure and index == 1
                for position in range(1, (1 if failed else count) + 1):
                    name = f"{index}-{position}.png"
                    archive.writestr(name, png)
                    steps = case["steps"] if position == count else []
                    stages.append(dict(
                        action="画面操作", screenshot=name,
                        checks=[dict(name="確認点", expected="A", actual="B" if failed else "A")],
                        spec=[dict(keyword=step["keyword"], text=step["text"]) for step in steps],
                    ))
                if wrong_spec and index == 1:
                    stages[-1]["spec"][-1]["text"] = "異なる期待結果"
                records.append(dict(name=case["name"], evidence_id=tag, steps=stages))
            if swap_tags:
                records[0]["evidence_id"], records[1]["evidence_id"] = (
                    records[1]["evidence_id"], records[0]["evidence_id"],
                )
            if swap_invalid_tags:
                records[5]["evidence_id"], records[7]["evidence_id"] = (
                    records[7]["evidence_id"], records[5]["evidence_id"],
                )
            archive.writestr("report.json", json.dumps(dict(scenarios=records)))
        return payload

    def read_evidence(self, **options):
        with tempfile.TemporaryDirectory() as destination, zipfile.ZipFile(self.evidence_zip(**options)) as archive:
            return report["read_report"](archive, Path(destination), self.catalogue)

    def render(self, evidence):
        return report["render"](self.catalogue, evidence, 55, "abc123", 123)

    def test_full_inventory_is_available_without_any_capture(self):
        markup = self.render({})
        self.assertEqual(markup.count("<section id='scenario-"), len(self.cases))
        self.assertEqual(markup.count("class='feature-group'"), len(self.catalogue))
        self.assertEqual(markup.count("<li class='step'"), sum(len(case["steps"]) for case in self.cases))
        self.assertIn("画面検査の結果 0 件", markup)
        self.assertNotIn("class='pass'", markup)
        self.assertNotIn("<img ", markup)
        self.assertIn("スクリーンショットなし", markup)

    def test_tagged_cases_show_screenshots_with_their_actual_coverage(self):
        evidence = self.read_evidence()
        self.assertEqual(set(evidence), {case["id"] for case in self.captured.values()})
        markup = self.render(evidence)
        self.assertEqual(markup.count("<img "), 23)
        self.assertEqual(markup.count("<strong class='pass'>成功"), 2)
        self.assertEqual(markup.count("<strong class='pass'>画面で確認"), 7)
        self.assertIn("MP4の出力と背景の画素は別途確認が必要です。", markup)
        self.assertIn("なぜ成功か", markup)
        for case in self.captured.values():
            scenario = f"scenario-{case['id']}"
            stages = len(evidence[case["id"]].stages)
            self.assertIn(f"href='#{scenario}' data-images='{stages}'", markup)
            panel = markup.split(f"<section id='{scenario}'", 1)[1].split("</section>", 1)[0]
            self.assertLess(panel.index("<img "), panel.index(f"id='{scenario}-spec'"))
            self.assertIn(f"href='#{scenario}-spec'", panel)

    def test_first_screen_failure_keeps_full_spec_and_opens_failed_case(self):
        evidence = self.read_evidence(first_failure=True)
        case = self.captured["ui-redo-after-edit"]
        self.assertTrue(evidence[case["id"]].failed)
        self.assertEqual(len(evidence[case["id"]].stages), 1)
        markup = self.render(evidence)
        self.assertIn(f'data-preferred="scenario-{case["id"]}"', markup)
        self.assertIn(case["steps"][-1]["text"], markup)
        self.assertIn("何が期待と違うか", markup)
        self.assertEqual(markup.count("<img "), 21)

    def test_changed_spec_or_swapped_tag_cannot_inherit_evidence(self):
        with self.assertRaisesRegex(ValueError, "title differs"):
            self.read_evidence(swap_tags=True)
        with self.assertRaisesRegex(ValueError, "steps differ"):
            self.read_evidence(wrong_spec=True)
        with self.assertRaisesRegex(ValueError, "steps differ"):
            self.read_evidence(swap_invalid_tags=True)

    def test_spec_arguments_are_rendered_as_text(self):
        content = "<script>alert(1)</script>"
        step = dict(background=False, keyword="Then", text=content)
        for argument in [
            dict(kind="doc_string", media_type="<test>", content=content),
            dict(kind="table", rows=[[content]]),
        ]:
            with self.subTest(argument=argument["kind"]):
                markup = report["render_spec"](dict(step, argument=argument))
                self.assertNotIn(content, markup)
                self.assertEqual(markup.count("&lt;script&gt;alert(1)&lt;/script&gt;"), 2)


if __name__ == "__main__":
    unittest.main()
