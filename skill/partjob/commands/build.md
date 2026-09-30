# build [targets] [--via actions|codebuild]

Produce the executables an order delivers. Only orders whose confirmed decisions call for executables use this subcommand; every other order skips it and `pack` carries no `program/` directory.

## Preconditions

- A numbered item in JOB.md "Confirmed decisions" names the target platforms (windows, linux, macos) and the build channel. Without it, stop and ask JC to settle it with `/partjob decide`; never infer targets from the client's operating system.
- The code to build is committed. Builds always use `HEAD`; uncommitted changes are not included.
- The PyInstaller build succeeds locally on Linux before any remote build is started.

## Channels

| Channel | When | Approval | Targets |
|---|---|---|---|
| `actions` (default) | A private GitHub repository for the order is acceptable | Pushing to the remote (approval item 2), once per push | windows, linux, macos |
| `codebuild` | The client's source must not go to any git host, or JC wants no repository for this order | Paid remote resources (approval item 4), once per build | windows only; Linux options in `references/codebuild.md` |

macOS builds go through `actions` only (`macos-latest`). CodeBuild macOS needs a reserved Mac fleet with a 24-hour minimum charge and is not used. A macOS binary is unsigned unless JC decides to pay for an Apple Developer account; unsigned is the default and goes into the manual's known limitations (the client opens it once with right-click, Open). `macos-latest` runs on Apple Silicon, so the binary is arm64 and does not run on Intel Macs unless JC decides on an Intel or universal build; the architecture is recorded in JOB.md and in the known limitations next to "unsigned".

## Steps

1. Read the decision; state targets and channel back to JC in one line.
2. First build of the project: copy the build files from the skill's `templates/build/` and replace `PROJECT_NAME` with the program name (the `--name` PyInstaller uses). gig does not render these files.
   - `actions`: `build.yml` to `.github/workflows/build.yml`; remove matrix entries for targets the decision does not name.
   - `codebuild`: `buildspec.yml` to the project root, `codebuild.ps1` to `packaging/`. Keep the script ASCII; non-ASCII text would require a UTF-8 BOM for Windows PowerShell 5.1.
   Commit them.
3. Start the build:
   - `actions`: ask JC for approval to push, push the commit (or the `vX.Y.Z` tag), then follow the run with `gh run watch`. Record the run id.
   - `codebuild`: ask JC for approval to use paid remote resources, then follow the "Each build" section of `references/codebuild.md`. Record the build id.
4. Fetch the artifacts into `dist-cloud/<target>/` (gitignored). Run `--version` and `--help` on the Linux binary locally. Windows and macOS binaries were smoke-tested inside the build only; say so.
5. Compute `sha256sum` for every artifact.
6. JOB.md "Status": channel, commit hash, run id or build id, each artifact with the first 12 characters of its sha256, what was only smoke-tested remotely.

## Reply

Targets and channel, the commit hash, the run or build id, where the artifacts landed, the sha256 prefixes, and that the next step is `/partjob pack`.
