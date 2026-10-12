# archive

After the warranty ends (or for a cancelled order): scorecard, then move the project directory to the archive root.

## Steps

1. Scorecard. Count the first four from JOB.md and this order's session memory; say 0 with a note when a count is not knowable. Ask JC for the score:
   - decisions: how many decisions JC made on this order (the number of "Confirmed decisions" entries is the lower bound).
   - repeat_questions: the sum of the counts the JOB.md "Status" entries record (see `log.md`).
   - cleanups: how many times JC cleaned up files by hand.
   - report_reworks: how many times a report was redone.
   - score: JC's 1 to 5 for how smoothly the order went.
   `gig scorecard --order <slug> --decisions N --repeat-questions N --cleanups N --report-reworks N --score S [--note "..."]`.
   `check_rejections` and `days_to_preview` are computed by gig.
2. Rehearse: `gig archive --order <slug>`. Read `blockers` (warranty not over, missing scorecard, wrong state), `git_dirty`, `large_files` (50 MB and up), `unsent_packages`.
3. List these for JC. Large files (checkpoints, data) are deleted only item by item on JC's decision; the deletion list is explicit; git history stays. Run `git ls-files <path>` on each deletion target first: tracked paths are listed and approved on their own.
4. After JC agrees: delete what was approved (only ignored content: `git clean -fdX <path>`) and commit; then `gig archive --order <slug> --yes` (warranty not over but JC wants to archive: add `--before-warranty-end`; no scorecard wanted: `--no-scorecard`; directory not wanted at all: `--purge`).
5. The archived project is at `~/Documents/archive/work/<slug>/` (archive_root in config).

## Reply

The scorecard, where the project went, what was deleted. Trends are visible through `gig ls --all` plus the scorecard in `gig show`.
