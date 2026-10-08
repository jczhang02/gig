"""Contract tests for the partjob skill.

They check that the skill stays internally consistent (router <-> command files,
gig commands named in the docs exist) and that gig renders the templates into
the file structure the workflow promises.
"""

import json
import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SKILL = (ROOT / "SKILL.md").read_text(encoding="utf-8")
COMMANDS_DIR = ROOT / "commands"
TEMPLATES = ROOT / "templates"
WORKFLOW = (ROOT / "references" / "workflow.md").read_text(encoding="utf-8")
GIG_REF = (ROOT / "references" / "gig.md").read_text(encoding="utf-8")

# gig v2 command surface, from docs/v2/SPEC.md section 5.
GIG_COMMANDS = {
    "draft new", "draft ls", "draft drop",
    "new", "ls", "show", "start", "change", "price", "note", "paid", "scorecard",
    "archive", "cancel", "cd", "delete",
    "package build", "package check", "package upload", "package sent", "package ls",
    "artifact upload", "artifact ls",
    "migrate", "doctor", "config get", "config set", "config path", "config split-secrets",
    "backup", "completion", "version",
}

ROUTER_ROW = re.compile(r"^\| `([a-z]+)")
CJK = re.compile(r"[一-鿿]")


def router_subcommands():
    names = []
    in_table = False
    for line in SKILL.splitlines():
        if line.startswith("| Subcommand"):
            in_table = True
            continue
        if in_table:
            m = ROUTER_ROW.match(line)
            if m:
                names.append(m.group(1))
            elif not line.startswith("|"):
                in_table = False
    return names


def gig_invocations(text):
    found = set()
    for m in re.finditer(r"`gig ([a-z-]+)(?: ([a-z-]+))?", text):
        first, second = m.group(1), m.group(2)
        if first in {"draft", "package", "artifact", "config"} and second:
            found.add(f"{first} {second}")
        else:
            found.add(first)
    return found


class SkillShape(unittest.TestCase):
    def test_frontmatter(self):
        head = SKILL.split("---", 2)[1]
        self.assertIn("name: partjob", head)
        self.assertIn("disable-model-invocation: true", head)
        self.assertIn("argument-hint:", head)

    def test_router_matches_command_files(self):
        names = router_subcommands()
        self.assertGreaterEqual(len(names), 15, names)
        files = {p.stem for p in COMMANDS_DIR.glob("*.md")}
        self.assertEqual(set(names), files)
        hint = SKILL.split("argument-hint")[1].split("\n")[0]
        for name in names:
            self.assertIn(name, hint, name)

    def test_skill_body_is_short(self):
        body = SKILL.split("---", 2)[2]
        self.assertLessEqual(len(body.splitlines()), 160)

    def test_skill_and_commands_are_english(self):
        for path in [ROOT / "SKILL.md", GIG_REF and ROOT / "references" / "gig.md", *COMMANDS_DIR.glob("*.md"), *(p for p in TEMPLATES.rglob("*") if p.is_file())]:
            text = path.read_text(encoding="utf-8")
            # Chinese is allowed only inside quoted examples and the note that clients read Chinese.
            stripped = re.sub(r"`[^`]*`", "", text)
            stripped = stripped.replace("Chinese", "")
            self.assertFalse(CJK.search(stripped), f"{path.name} contains Chinese prose")

    def test_approval_list_and_generic_rules_present(self):
        for needle in ("Actions only JC can approve", "Sending anything out", "Pushing to a remote", "Deleting files",
                       "paid remote resources", "Scope changes", "QUOTE.md", "--yes"):
            self.assertIn(needle, SKILL)
        for needle in (".gig/JOB.md", "read-only", "byte for byte", ".scratch/", "ASCII punctuation", "humanizer"):
            self.assertIn(needle, SKILL)

    def test_plain_language_routing_is_documented(self):
        self.assertIn("not a subcommand name", SKILL)
        self.assertIn("closest subcommand", SKILL)

    def test_command_files_only_name_real_gig_commands(self):
        for path in sorted(COMMANDS_DIR.glob("*.md")):
            text = path.read_text(encoding="utf-8")
            unknown = gig_invocations(text) - GIG_COMMANDS
            self.assertFalse(unknown, f"{path.name} names unknown gig commands: {unknown}")

    def test_gig_reference_covers_every_command(self):
        for cmd in GIG_COMMANDS:
            self.assertIn(f"gig {cmd}", GIG_REF, cmd)

    def test_yes_gated_commands_are_rehearsed_first(self):
        send = (COMMANDS_DIR / "send.md").read_text(encoding="utf-8")
        self.assertIn("gig package upload <package-id>`", send)
        self.assertIn("--yes", send)
        self.assertIn("Rehearse", send)
        self.assertIn("to the phone", send)
        archive = (COMMANDS_DIR / "archive.md").read_text(encoding="utf-8")
        self.assertIn("gig archive --order <slug>`", archive)
        self.assertIn("--yes", archive)
        drop = (COMMANDS_DIR / "drop.md").read_text(encoding="utf-8")
        self.assertIn("would_delete", drop)

    def test_send_offers_pan_first_and_reports_the_extraction_code(self):
        send = (COMMANDS_DIR / "send.md").read_text(encoding="utf-8")
        self.assertIn("--via pan|oss|phone", send.splitlines()[0])
        self.assertIn("`网盘, OSS, or phone?`", send)
        self.assertIn("--uploader bdpan --yes", send)
        self.assertIn("--uploader s3:", send)
        self.assertIn("`pwd`", send)
        self.assertIn("! bdpan login", send)
        self.assertIn(r"`send <package-id> [--via pan\|oss\|phone]`", SKILL)
        self.assertIn(r"`send <id> [--via pan\|oss\|phone]`", WORKFLOW)

    def test_gig_reference_names_the_uploader_override(self):
        for cmd in ("gig package upload <package-id> [--order <slug>] [--uploader bdpan|s3:<name>] [--yes]",
                    "gig artifact upload <FILE> [--order <slug>] [--uploader bdpan|s3:<name>] [--yes]"):
            self.assertIn(cmd, GIG_REF)
        self.assertIn("`pwd`", GIG_REF)

    def test_workflow_doc_has_no_open_items(self):
        self.assertNotIn("[TODO", WORKFLOW)
        for section in ("## 1. Process", "## 2. JOB.md and QUOTE.md", "## 6. Skill subcommands", "## Appendix C. Acceptance criteria"):
            self.assertIn(section, WORKFLOW)

    def test_templates_exist(self):
        for name in ("NOTES.md.j2", "JOB.md.j2", "QUOTE.md.j2", "AGENTS.md.j2", "README.md.j2", "gitignore"):
            self.assertTrue((TEMPLATES / name).is_file(), name)
        gitignore = (TEMPLATES / "gitignore").read_text(encoding="utf-8")
        for line in ("delivery/", ".scratch/", "data/samples/"):
            self.assertIn(line, gitignore)


def gig_bin():
    candidate = Path(os.environ.get("GIG_BIN", Path.home() / "Documents/dev-tools/gig/target/debug/gig"))
    if not candidate.is_file():
        raise AssertionError(f"gig binary not found at {candidate}; build gig v2 or set GIG_BIN")
    return candidate


class TemplatesRenderThroughGig(unittest.TestCase):
    def run_gig(self, home, *args, cwd=None):
        env = dict(os.environ, GIG_HOME=str(home), GIG_GENERAL_TEMPLATES_DIR=str(TEMPLATES))
        out = subprocess.run([str(gig_bin()), *args], env=env, cwd=cwd, capture_output=True, text=True)
        self.assertTrue(out.stdout.strip(), f"no output for {args}: {out.stderr}")
        doc = json.loads(out.stdout)
        self.assertTrue(doc["ok"], f"{args}: {doc}")
        return doc["data"]

    def test_draft_then_new_renders_promised_structure(self):
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp) / "home"
            dev = Path(tmp) / "dev"
            dev.mkdir()
            (home / "config").mkdir(parents=True)
            (home / "config" / "config.toml").write_text(
                f'[general]\ndev_root = "{dev}"\narchive_root = "{Path(tmp) / "archive"}"\n', encoding="utf-8"
            )
            draft = self.run_gig(home, "draft", "new", "pdf-tool", "--title", "PDF tool", "--material", "/mnt/virtiofs/000001")
            notes = Path(draft["notes_path"])
            text = notes.read_text(encoding="utf-8")
            for h in ("## Client words", "## Questions", "## Feasibility", "## Effort estimate", "## Budget and pricing"):
                self.assertIn(h, text)
            self.assertIn("/mnt/virtiofs/000001", text)
            notes.write_text(text + "\nClient budget 800.\n", encoding="utf-8")

            created = self.run_gig(
                home, "new", "pdf-tool", "--title", "PDF tool", "--price", "800", "--type", "tool",
                "--client-words", "报价 800, 已成交", "--from-draft",
            )
            self.assertEqual(created["order"]["status"], "queued")
            project = dev / "pdf-tool"
            job = (project / ".gig/JOB.md").read_text(encoding="utf-8")
            for h in ("## Now", "## Client request", "## Pre-order notes", "## Material facts", "## Confirmed decisions", "## Client questions", "## Status"):
                self.assertIn(h, job)
            self.assertIn("> 报价 800, 已成交", job)
            self.assertIn("Client budget 800.", job)
            self.assertIn("/mnt/virtiofs/000001", job)
            self.assertIn("Not sent out yet", job)
            quote = (project / ".gig/QUOTE.md").read_text(encoding="utf-8")
            self.assertIn("CNY 800.00", quote)
            self.assertIn("not received", quote)
            self.assertIn("Warranty: 15 days after payment", quote)
            self.assertIn("Not agreed", quote)
            agents = (project / "AGENTS.md").read_text(encoding="utf-8")
            self.assertIn("## Tool projects", agents)
            self.assertIn("/mnt/virtiofs/000001", agents)
            self.assertIn("## Project-specific", agents)
            gitignore = (project / ".gitignore").read_text(encoding="utf-8")
            self.assertIn("delivery/", gitignore)
            self.assertTrue((project / "README.md").is_file())
            self.assertTrue((project / "data").is_dir())
            self.assertFalse(notes.exists())
            for stray in ("PLAN.md", "ACCEPTANCE.md", "INDEX.html", "progress-log.md"):
                self.assertFalse((project / ".gig" / stray).exists(), stray)

            shown = self.run_gig(home, "show", cwd=project)
            self.assertEqual(shown["order"]["slug"], "pdf-tool")
            self.assertEqual(shown["next_action"], "start")

    def test_cv_ml_gets_its_own_agents_template(self):
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp) / "home"
            dev = Path(tmp) / "dev"
            dev.mkdir()
            (home / "config").mkdir(parents=True)
            (home / "config" / "config.toml").write_text(f'[general]\ndev_root = "{dev}"\n', encoding="utf-8")
            self.run_gig(home, "new", "spectra", "--title", "Spectra", "--type", "cv_ml")
            agents = (dev / "spectra" / "AGENTS.md").read_text(encoding="utf-8")
            self.assertIn("## CV / ML projects", agents)
            self.assertIn("remote GPU", agents)


if __name__ == "__main__":
    unittest.main()
