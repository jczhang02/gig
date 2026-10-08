#!/usr/bin/env bash
# Build the Windows artifacts of a partjob project on the local libvirt VM (/partjob build --via vm).
# Runbook: references/vm.md. Usage: vm-build.sh [--keep] <project-dir>
#
# Archives the committed HEAD, copies it to the VM, runs packaging/build-windows.ps1 there, and fetches the
# regular files in the VM's dist/ into <project>/dist-cloud/windows/. The full log goes to
# <project>/.scratch/vm-build.log. The VM work directory C:\pj\<name> is removed after a successful build and
# kept after a failed one (or with --keep) for debugging. A VM the script had to start is shut down at the end.
set -euo pipefail

VM=${PARTJOB_VM:-win11}           # libvirt domain name and ssh host alias
LIBVIRT=qemu:///system
BOOT_WAIT=${PARTJOB_VM_BOOT_WAIT:-240}

keep=0
if [[ ${1:-} == --keep ]]; then keep=1; shift; fi
[[ $# -eq 1 ]] || { echo "usage: vm-build.sh [--keep] <project-dir>" >&2; exit 2; }
proj=$(cd "$1" && pwd)
name=$(basename "$proj")
[[ $name =~ ^[A-Za-z0-9._-]+$ ]] || { echo "project directory name must be ASCII letters, digits, . _ -: $name" >&2; exit 2; }

git -C "$proj" ls-tree --name-only HEAD packaging/build-windows.ps1 | grep -q . \
  || { echo "packaging/build-windows.ps1 is not committed in $proj (copy it from templates/build/ first)" >&2; exit 1; }
commit=$(git -C "$proj" rev-parse HEAD)
if [[ -n $(git -C "$proj" status --porcelain --untracked-files=no) ]]; then
  echo "note: uncommitted changes are not included; building HEAD only"
fi

mkdir -p "$proj/.scratch"
log="$proj/.scratch/vm-build.log"
exec > >(tee "$log") 2>&1
echo "project $name, commit $commit, VM $VM, $(date -Is)"

# PowerShell over ssh: cmd.exe is the remote shell, so pass PowerShell as base64 UTF-16LE and avoid quoting.
# Progress records would otherwise arrive as CLIXML on stderr. stderr joins stdout so CRLF can be cut to LF.
remote_ps() {
  local encoded
  encoded=$(printf '%s' "\$ProgressPreference = 'SilentlyContinue'; $1" | iconv -f UTF-8 -t UTF-16LE | base64 -w0)
  ssh "$VM" "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand $encoded" 2>&1 | sed -u 's/\r$//'
}

# A VM this script started is shut down again on exit, after a failure too; one that was already running is left alone.
started=0
src=$(mktemp -d)
cleanup() {
  rm -rf "$src"
  if ((started)); then
    echo "shutting the VM down"
    virsh -c "$LIBVIRT" shutdown "$VM" >/dev/null || true
  fi
}
trap cleanup EXIT

state=$(virsh -c "$LIBVIRT" domstate "$VM")
if [[ $state != running ]]; then
  echo "VM is $state; starting it"
  virsh -c "$LIBVIRT" start "$VM" >/dev/null
  started=1
fi
for ((waited = 0; ; waited += 5)); do
  ssh -o ConnectTimeout=5 -o BatchMode=yes "$VM" exit 2>/dev/null && break
  ((waited < BOOT_WAIT)) || { echo "no ssh to $VM after ${BOOT_WAIT} s" >&2; exit 1; }
  sleep 5
done

work="C:\\pj\\$name"
git -C "$proj" archive --format=zip -o "$src/src.zip" HEAD
remote_ps "New-Item -ItemType Directory -Force C:\\pj | Out-Null"
scp -q "$src/src.zip" "$VM:C:/pj/$name-src.zip"

start=$(date +%s)
build_status=0
remote_ps "
\$ErrorActionPreference = 'Stop'
if (Test-Path '$work') { Remove-Item -Recurse -Force '$work' }
Expand-Archive 'C:\\pj\\$name-src.zip' '$work'
Set-Location '$work'
powershell -NoProfile -ExecutionPolicy Bypass -File packaging\\build-windows.ps1
exit \$LASTEXITCODE
" || build_status=$?
echo "build on VM: $(($(date +%s) - start)) s, exit $build_status"

if ((build_status == 0)); then
  mapfile -t files < <(remote_ps "Get-ChildItem -File -ErrorAction SilentlyContinue '$work\\dist' | ForEach-Object Name" | grep .)
  ((${#files[@]})) || { echo "no files in $work\\dist" >&2; build_status=1; }
fi
if ((build_status == 0)); then
  out="$proj/dist-cloud/windows"
  mkdir -p "$out"
  for f in "${files[@]}"; do scp -q "$VM:C:/pj/$name/dist/$f" "$out/"; done
  echo "artifacts in $out:"
  (cd "$out" && sha256sum "${files[@]}")
fi

if ((keep || build_status)); then
  echo "kept $work on the VM"
else
  remote_ps "Remove-Item -Recurse -Force -ErrorAction SilentlyContinue '$work', 'C:\\pj\\$name-src.zip'"
fi
exit "$build_status"
