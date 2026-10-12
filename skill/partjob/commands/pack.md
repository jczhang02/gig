# pack [version]

The full package, sent after the client accepts the work.

## Contents (defaults per project type; confirm per order in JOB.md)

- Any job: the delivery report PDF (installation, usage, results, known limitations, warranty terms), source code (`git archive`, without `.gig/`, `.scratch/` and client samples), README.
- Tool jobs add: the executables the order's decisions call for, produced by `/partjob build` (no `program/` directory when the order needs no program), this batch's processed results, a sample configuration file.
- Reproduction / analysis jobs add: configurations, result data, figures, the differences or blockers report.
- Setup jobs (`custom` orders that configure the client's tools, often finished in a remote session) add: an install manual written for the client, the scripts and configuration files it uses; no `source.zip` unless a decision asks for it. Their reproduction check (step 2) is an end-to-end install and uninstall by the manual in a clean environment (a fresh user, VM or container).
- Writing jobs add: PDF plus sources. A full delivery of a single PDF is still a full package sent with `send`, never `gig artifact upload`, so the order moves to delivered.
- Never: copies of the client's originals, intermediates (content.json, html, md, -visual directories, build scripts), internal audit files, `reports/internal/`, data beyond test data.

## Steps

1. Version: v1.0.0 for the first delivery; bump minor or patch after rework. The package id is `<slug>-vX.Y.Z`. Whether the previous package directory is deleted is JC's call.
2. Reproduction check (mandatory before delivery; results go into JOB.md):
   - Tool jobs: rerun this batch's inputs with the program in the delivery directory (the build recorded by `/partjob build`) and compare byte for byte with the source run. The recorded build commit must equal the `HEAD` archived in step 4; otherwise state the mismatch and rebuild.
   - Reproduction / analysis jobs: rerun the key results with the delivered configuration; they must match the report's numbers.
   - A Windows program built with `--via vm`: run the delivered exe on this batch's inputs in the VM and compare with a source run in the same VM (`references/vm.md`, "Reproduction check"). A Linux source run is not the comparison: line endings and image decoding differ across platforms.
   - Outputs keep the original's format and metadata (for images: colour profile chunks such as sRGB and gAMA), apart from the changes the decisions ask for.
   - A Windows or macOS program only smoke-tested in the remote build is stated as such in JOB.md and in the report's known limitations; so is an unsigned macOS program. A check JC waives is recorded the same way.
3. Report: sources in `.scratch/reports/<name>/`, generated with Kami or LaTeX (a patch version edits the previous source in place, `content.json` kept in sync, instead of regenerating it), prose through the humanizer skill (humanizer-zh for Chinese; run the skill, a self-check does not replace it), only the PDF goes into the package. The report never claims a workflow state or an approval.
4. Source: `git archive --format=zip -o delivery/<package-id>/source.zip HEAD`. `.gitattributes` (from the template) marks `.gig`, `AGENTS.md` and the other project records `export-ignore`; a project without it gets it first, committed. Confirm `.gitignore` excludes client samples; `gig package check` rejects a `source.zip` that carries `.gig/` or other internal files.
   When the README needs the source unpacked at the package root (for example beside `results/`), extract the same `git archive` output there instead of shipping `source.zip`.
5. English file names throughout: `manual.pdf`, `report.pdf`, `source.zip`, `program/<name>.exe`, `program/<name>` (Linux), `program/<name>-macos`, `results/`.
6. Before building, list the top-level entries of `delivery/<package-id>/` with their sizes (`du -sh`) and one reason each goes to the client; flag anything over 100 MB. Build after JC agrees to the list.
7. `gig package build <package-id> --write-manifest [--client-named results/]`. Client-named result files (Chinese, spaces) live in a subdirectory such as `results/` declared with `--client-named`; `check` lists every exempted file as a warning, so confirm at review time that they carry names the client gave, or names in the client's language a confirmed decision asks for. Other program output names are English.
8. Show JC `data.files` and `warnings`. Every file in the package must have a reason to go to the client.
9. JOB.md "Status": package id, file count, first 12 characters of the sha256, reproduction check result, "not sent out yet".

## Reply

The package id, the absolute paths of the package directory and zip, the file list, verification results, warnings, the `.scratch/` size (`du -sh .scratch`) with one question whether to clear it, and that the next step is `/partjob send <package-id>`.
