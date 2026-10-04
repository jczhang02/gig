# start <slug>

The client placed the order; the job officially begins. Register the order, create the project directory, set up agent rules, then go to grill.

## Preconditions

- From JC: the agreed price (major units, e.g. 800), the client's or JC's words confirming the deal (verbatim), the project type (`tool`, `cv_ml`, `data_processing`, `research_writing`, `custom`), the material path (not needed again if the draft has it).
- A draft with the same slug exists: use `--from-draft`; its notes land in JOB.md under "Pre-order notes".
- The directory already exists (a project set up by hand earlier): use `--adopt --status in_progress`, which touches no file; fill in whatever `.gig/JOB.md` and `.gig/QUOTE.md` lack afterwards.

## Steps

1. Pass the client's words with `--client-words "<words>"`; if long, write them to a file under `/tmp` first and pass `--client-words @file`.
2. `gig new <slug> --title "<title>" --price <amount> --type <type> --material <path> [--platform <platform>] --client-words ... [--from-draft]`.
   The result lists `created_files` and `warnings` (git being unavailable shows here). The directory is `~/dev/partjobs/<slug>/` with `.gig/JOB.md`, `.gig/QUOTE.md`, `AGENTS.md`, `README.md`, `.gitignore`, `data/`, `references/`.
   Claude Code only: if the session's working directory is not `~/dev/partjobs/<slug>/`, stop and ask JC to run `/cd ~/dev/partjobs/<slug>`; continue from step 3 after the move. Other agents skip this.
3. Read the generated `.gig/JOB.md` and `.gig/QUOTE.md` and fill every "(to fill)" with known facts. The price and words in QUOTE.md must match what JC said.
4. In the project directory run the `setup-matt-pocock-skills` skill (issue tracker, triage labels, domain docs). It appends an "Agent skills" section to AGENTS.md; keep the template content.
5. Fill the "Project-specific" section of AGENTS.md for this project type: read-only paths, whether training is allowed and where, local toolchain, whether continuous work is authorised, git authorisation. Leave what is unknown and ask in the grill.
6. `gig start --order <slug>`. Status becomes `in_progress`.
7. Tell JC the next step is `grill-me` (small job) or `grill-with-docs` (a job that needs CONTEXT.md and ADRs) to settle the route. Grill conclusions go into JOB.md one by one via `/partjob decide`.
8. First commit: the skeleton files. No push.

## Reply

Which files were created, the price and date in QUOTE.md, which "(to fill)" items remain in AGENTS.md, and that the next step is grill.
