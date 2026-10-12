# Building Windows executables on the local VM

Used by `commands/build.md` when the channel is `vm`: the source never leaves JC's machine and nothing is billed. `scripts/vm-build.sh` zips the committed tree with `git archive`, copies it to a Windows 11 virtual machine over SSH, runs `packaging/build-windows.ps1` there, and fetches the artifacts back.

Verified 2026-10-08:
- thermal-video-curve (PySide6, OpenCV, matplotlib, onedir), with a trial script doing the same steps: 105 s with empty caches, 81 s with warm caches, tests and the window smoke test passed.
- A throwaway onefile console project with `vm-build.sh` and the `build-windows.ps1` template: success, artifact fetch, work directory removal; a failing test stopped the build with exit 1 and kept the work directory.
- Starting from a shut-off VM: the same onefile project built in 31 to 35 s in total, boot and SSH wait included; the script shut the VM down afterwards (off 10 s later). A VM that was already running stayed running.

## Provisioned resources (on JC's workstation)

| Resource | Name | Configuration |
|---|---|---|
| libvirt domain | `win11` on `qemu:///system` | Windows 11 Enterprise LTSC, 4 vCPU, 8 GB, about 25 GB free on C:. No autostart |
| Address | `192.168.122.80` | libvirt `default` NAT network, reserved for MAC `52:54:00:1c:45:cf` in its DHCP |
| SSH host alias | `win11` in `~/.ssh/config.d/win11` | User `jczhang`, key `~/.ssh/id_ed25519_win11`. Machine-specific, not in dotfiles |
| SSH server | OpenSSH Server in Windows | Key in `C:\ProgramData\ssh\administrators_authorized_keys` (the account is an administrator). Firewall rule `OpenSSH-Server-In-TCP` set to profile Any, remote address `192.168.122.1` only |
| Work area | `C:\pj\<project>` | Created per build, removed after success, kept after failure |
| Kept between builds | `C:\uv\uv.exe`, uv cache and uv Python under the user profile | About 450 MB after one PySide6 project; makes warm builds skip about 15 s of downloads |

Nothing here is paid or reachable from outside the workstation, so the channel needs none of JC's approvals. The script deletes only its own work directory on the VM.

## One-time setup in Windows

Already done on `win11`. For a new or reinstalled VM, in an administrator PowerShell:

```powershell
Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0
Start-Service sshd
Set-Service sshd -StartupType Automatic
# The VM network is classified Public; the default rule does not cover it. Allow the host only.
if (-not (Get-NetFirewallRule -Name OpenSSH-Server-In-TCP -ErrorAction SilentlyContinue)) {
  New-NetFirewallRule -Name OpenSSH-Server-In-TCP -DisplayName 'OpenSSH Server' -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22
}
Set-NetFirewallRule -Name OpenSSH-Server-In-TCP -Profile Any -RemoteAddress 192.168.122.1
$k = '<contents of ~/.ssh/id_ed25519_win11.pub>'
Add-Content -Force -Path C:\ProgramData\ssh\administrators_authorized_keys -Value $k
icacls.exe C:\ProgramData\ssh\administrators_authorized_keys /inheritance:r /grant "Administrators:F" /grant "SYSTEM:F"
```

Pause Windows Update (or set active hours) so the VM does not restart during a build; on `win11` JC paused it on 2026-10-08. A pause runs out after some weeks, so renew it when Windows offers updates again. The VM needs no Python, git or Visual Studio; the build script installs uv and Python itself.

## Files the project needs

From the skill's `templates/build/`: `build-windows.ps1` in `packaging/`, with `PROJECT_NAME` replaced, committed. It is the same script CodeBuild runs. The artifacts are the regular files directly in `dist/`: the exe for onefile builds; for onedir builds the script must zip the folder with `tar.exe -a -c -f dist\<name>-windows.zip -C dist <name>` (see the script header). Project-specific steps (onedir, `--windowed`, a window smoke test with `QT_QPA_PLATFORM=offscreen`) go into the project's copy, not the template.

## Each build

```bash
~/.agents/skills/partjob/scripts/vm-build.sh ~/dev/partjobs/<slug>
```

What it does: checks that `packaging/build-windows.ps1` is committed, notes uncommitted changes (they are not built), starts the VM when it is not running and waits up to 240 s for SSH, copies `git archive HEAD` to `C:\pj\<slug>-src.zip`, unpacks it into a fresh `C:\pj\<slug>`, runs the build script, fetches every regular file of `dist\` into `dist-cloud/windows/` (same names are overwritten), prints their sha256, and removes the work directory. When the script started the VM, it shuts it down again on exit (`virsh shutdown`, after a failure too); a VM that was already running, for example because JC is using it, is left running. The whole output is in `.scratch/vm-build.log`; the commit hash is its first line. The exit code is the build's.

Options: `--keep` leaves the work directory after success too. `PARTJOB_VM` selects another domain and SSH alias, `PARTJOB_VM_BOOT_WAIT` the boot wait in seconds.

A failed build leaves `C:\pj\<slug>` in place. Look at it with `ssh win11`; the next build of the same project removes it.

## Reproduction check

`pack` compares the delivered exe with a source run on the same VM, never with a Linux run. Start the VM first (`virsh -c qemu:///system start win11`) so the script leaves it running, run `vm-build.sh --keep`, copy this batch's inputs into the kept work directory over `ssh win11`, run the source (`uv run ...`) and the exe from `dist\` on them into two output directories, and compare their sha256 lists. Remove the inputs and outputs afterwards: they are client data. Record the result in JOB.md, and shut the VM down when it was off before.

## Time

Measured with thermal-video-curve: upload 1 s, uv, Python and dependencies 16 s cold or 1 s warm, tests 8 s, PyInstaller 53 to 60 s, smoke test 4 s, zip 9 s (`tar.exe`), fetch of a 113 MB zip about 1 s. The same project took 196 s on GitHub Actions (PyInstaller alone 112 s); CodeBuild took 136 s for the smaller tk-dtf-compact. The VM shares the workstation CPU, so builds slow down when the host is busy.

## Differences from the cloud channels

| Difference | Effect |
|---|---|
| No Windows SDK in the VM, so PyInstaller does not bundle the UCRT DLLs (`api-ms-win-*.dll`, `ucrtbase.dll`) that the GitHub runner build carries | None on Windows 10 and later, which ship UCRT; Qt 6 needs Windows 10 anyway. A program that must run on Windows 7 or 8.1 uses another channel |
| Windows PowerShell 5.1, not PowerShell 7 | `Compress-Archive` writes backslash paths; zip with `tar.exe`. Console output over SSH is GBK unless the script sets UTF-8 (the template does) |
| A real Windows 11 desktop OS, not a Server Core container | The smoke test runs on the same kind of system the client uses; window programs still need `offscreen` because the SSH session has no desktop |
| Windows activation and licensing | The VM's licence state is JC's responsibility; nothing in the build depends on it |

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| Port 22 times out (not refused), ping fails | Windows firewall drops it: the network profile is Public and the rule does not cover it. Run the `Set-NetFirewallRule` line above |
| `no ssh to win11 after 240 s` | The VM is still booting or updating; check with `virt-viewer` or `virsh -c qemu:///system domstate win11` |
| `Host key verification failed` | The VM was reinstalled: `ssh-keygen -R 192.168.122.80`, then `ssh win11 exit` once |
| `Permission denied (publickey)` | The key file lost its ACL or is in the wrong place; redo the `administrators_authorized_keys` lines |
| `#< CLIXML` or garbled Chinese in output | A PowerShell command run outside `remote_ps`; use the script, or set `$ProgressPreference` and the console encoding |
| Disk full | Each project work directory is about 1 GB while it exists; `uv cache clean` in the VM frees the cache |

## Teardown

Irreversible; only when JC decides the channel is no longer needed. The VM itself stays (it is JC's).

```bash
rm ~/.ssh/config.d/win11 ~/.ssh/id_ed25519_win11 ~/.ssh/id_ed25519_win11.pub
virsh -c qemu:///system net-update default delete ip-dhcp-host "<host mac='52:54:00:1c:45:cf' name='win11' ip='192.168.122.80'/>" --live --config
```

In the VM: `Stop-Service sshd; Set-Service sshd -StartupType Disabled`, remove the key line, delete `C:\pj` and `C:\uv`.
