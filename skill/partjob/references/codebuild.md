# Building Windows executables on AWS CodeBuild

Used by `commands/build.md` when the channel is `codebuild`: the source never reaches a git host. The committed tree is zipped locally with `git archive`, uploaded to S3, built in a Windows container, and the exe is fetched back from S3.

Verified 2026-09-29 with tk-dtf-compact v1.3.0: 2 min 16 s from start to finish, a 64 MB artifact, and the remote smoke test printed the version.

## Provisioned resources (one set per AWS account, region us-west-2)

Shared by every order; nothing is created per project. The account id is not written in this public repository; get it with `aws sts get-caller-identity --query Account --output text` and the bucket with `aws s3 ls | grep partjobs-build`.

| Resource | Name | Configuration |
|---|---|---|
| S3 bucket | `partjobs-build-<account-id>-us-west-2` | Public access blocked, objects expire after 7 days |
| IAM role | `partjobs-codebuild` | Read and write on that bucket, write build logs |
| CodeBuild project | `partjobs-win` | `WINDOWS_SERVER_2022_CONTAINER`, image `aws/codebuild/windows-base:2022-1.0`, `BUILD_GENERAL1_MEDIUM` (4 vCPU, 8 GB) |
| Log group | `/aws/codebuild/partjobs-win` | 14-day retention |
| Budget | `partjobs-build` | CodeBuild 5 USD per month; mail at 80% actual or 100% forecast |

Limits: 30-minute build timeout, 30-minute queue timeout, one concurrent build (the account quota is 15, no request needed). The project's configured source is a placeholder: **every start must pass `--source-location-override`**.

Linux: no CodeBuild project is provisioned. An order that forbids git hosting and also needs a Linux binary either builds it locally (record the glibc version in the known limitations, since a binary built on a rolling distribution does not run on older systems) or JC provisions a `partjobs-linux` project first (Linux builds are free-tier eligible).

macOS: not through CodeBuild. It requires a reserved Mac fleet billed for at least 24 hours per instance. Use GitHub Actions `macos-latest` instead, see `commands/build.md`.

## Files the project needs

From the skill's `templates/build/`: `buildspec.yml` at the project root, `codebuild.ps1` in `packaging/`, with `PROJECT_NAME` replaced. The container has no Python; the script installs uv, Python 3.11 and the dependencies, runs the tests, PyInstaller, and a smoke test.

- PowerShell does not stop when an external command fails; the `Step` function checks `$LASTEXITCODE`. Pure PowerShell steps (installing uv) never set it, so it is reset to 0 before each step. The first verification run failed on exactly this.
- Keep the script ASCII. Non-ASCII text needs a UTF-8 BOM for Windows PowerShell 5.1.
- `no matching artifact paths found` means an earlier step failed, or the artifact path in `buildspec.yml` is wrong.

## Each build

Prerequisite: `aws login` not expired. Set the region explicitly on every command; the default in `~/.aws/config` has been an invalid availability-zone name before (fixing it once with `aws configure set region us-west-2` is better than carrying the workaround).

```bash
export AWS_REGION=us-west-2
B=partjobs-build-<account-id>-us-west-2
NAME=<slug>                               # only used for the S3 paths
cd ~/dev/partjobs/$NAME

# 1) zip the committed tree and upload (uncommitted changes are not included)
mkdir -p .scratch
git archive --format=zip -o .scratch/$NAME-src.zip HEAD
aws s3 cp .scratch/$NAME-src.zip s3://$B/src/$NAME.zip

# 2) start; note the build id it prints
aws codebuild start-build --project-name partjobs-win \
  --source-type-override S3 \
  --source-location-override $B/src/$NAME.zip \
  --artifacts-override "type=S3,location=$B,path=builds/$NAME,namespaceType=BUILD_ID,name=out,packaging=NONE" \
  --query build.id --output text
ID=partjobs-win:xxxxxxxx-...

# 3) poll until the status is no longer IN_PROGRESS
aws codebuild batch-get-builds --ids $ID --query 'builds[0].[buildStatus,currentPhase]' --output text

# 4) logs (the last lines, on failure)
aws logs tail /aws/codebuild/partjobs-win --log-stream-names ${ID#*:} --since 3h --format short | tail -80

# 5) fetch the artifacts
LOC=$(aws codebuild batch-get-builds --ids $ID --query 'builds[0].artifacts.location' --output text)
aws s3 cp --recursive "s3://${LOC#arn:aws:s3:::}/" dist-cloud/windows/

# 6) remove the uploaded source (it expires after 7 days anyway)
aws s3 rm s3://$B/src/$NAME.zip
rm .scratch/$NAME-src.zip
```

The `HEAD` zipped in step 1 must be the commit `pack` later archives as `source.zip`. Record the hash in JOB.md.

## Time and cost

Measured with tk-dtf-compact: queue 0 s (4 min 26 s on the first build of the day, a cold Windows container), provisioning 47 s, source download 9 s, build 70 s, artifact upload 6 s.

Billing is per build minute, rounded up. Windows medium is about 0.018 USD per minute (a third-party figure; the AWS pricing page does not list it, so check the bill). About 0.05 USD per build, at most about 0.54 USD for a build that runs into the 30-minute timeout. Failed builds are billed. The free tier does not include Windows. S3 and log costs are negligible.

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `SSL validation failed for https://sts.<zone>.amazonaws.com/` | The default region is an availability-zone name. `export AWS_REGION=us-west-2`, or fix `~/.aws/config` |
| `Your session has expired` | Run `aws login` |
| Stuck in QUEUED | Cold Windows container, or another build is running (concurrency 1). Cancelled after 30 minutes |
| BUILD fails within seconds | Read the log (step 4). Usually the exit-code check or the file encoding, see above |
| `no matching artifact paths found` | An earlier step failed, or the artifact path in `buildspec.yml` is wrong |

## Teardown

Irreversible; only when JC decides the channel is no longer needed.

```bash
export AWS_REGION=us-west-2
aws codebuild delete-project --name partjobs-win
aws iam delete-role-policy --role-name partjobs-codebuild --policy-name build
aws iam delete-role --role-name partjobs-codebuild
aws s3 rb --force s3://partjobs-build-<account-id>-us-west-2
aws logs delete-log-group --log-group-name /aws/codebuild/partjobs-win
aws budgets delete-budget --account-id <account-id> --budget-name partjobs-build
```
