# pack [version]

The full package, sent after the client accepts the work.

## Contents (defaults per project type; confirm per order in JOB.md)

- Any job: the delivery report PDF (installation, usage, results, known limitations, warranty terms), source code (`git archive`, without `.gig/`, `.scratch/` and client samples), README.
- Tool jobs add: executables (Windows and Linux), this batch's processed results, a sample configuration file.
- Reproduction / analysis jobs add: configurations, result data, figures, the differences or blockers report.
- Writing jobs add: PDF plus sources.
- Never: copies of the client's originals, intermediates (content.json, html, md, -visual directories, build scripts), internal audit files, data beyond test data.

## Steps

1. Version: v1.0.0 for the first delivery; bump minor or patch after rework. The package id is `<slug>-vX.Y.Z`. Whether the previous package directory is deleted is JC's call.
2. Reproduction check (mandatory before delivery; results go into JOB.md):
   - Tool jobs: rerun this batch's inputs with the program in the delivery directory (the CI build) and compare byte for byte with the source run.
   - Reproduction / analysis jobs: rerun the key results with the delivered configuration; they must match the report's numbers.
   - A Windows program only smoke-tested on CI is stated as such in JOB.md and in the report's known limitations.
3. Report: sources in `.scratch/reports/<name>/`, generated with Kami or LaTeX, prose through sepia, only the PDF goes into the package. The report never claims a workflow state or an approval.
4. Source: `git archive --format=zip -o delivery/<package-id>/source.zip HEAD`; confirm `.gitignore` excludes client samples; spot-check with `unzip -l` that no `.gig/` is inside.
5. English file names throughout: `manual.pdf`, `report.pdf`, `source.zip`, `program/<name>.exe`, `program/<name>` (Linux), `results/`.
6. `gig package build <package-id> --write-manifest [--client-named results/]`. Client-named result files (Chinese, spaces) live in a subdirectory such as `results/` declared with `--client-named`; `check` lists every exempted file as a warning, so confirm at review time that they really carry the client's own names.
7. Show JC `data.files` and `warnings`. Every file in the package must have a reason to go to the client.
8. JOB.md "Status": package id, file count, first 12 characters of the sha256, reproduction check result, "not sent out yet".

## Reply

The package id, the file list, verification results, warnings, and that the next step is `/partjob send <package-id>`.
