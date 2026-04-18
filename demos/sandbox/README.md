# Sandbox Demo

Ephemeral VMs on demand — spin up a sandbox from a template, use it, and let it auto-expire.

A **sandbox** is a single-use VM with an idle timeout.  When no one has fetched
the sandbox's status within the timeout window, the reaper automatically stops
and deletes the underlying VM.  No manual cleanup required.

## What this demo shows

1. Create two VM templates backed by the same boot source (Firecracker default + Cloud Hypervisor comparison)
2. Provision one sandbox from each template and measure time-to-ready
3. Inspect the Firecracker sandbox that represents the default sandbox path
4. Execute a command inside the Firecracker sandbox over the guest agent
5. Provision a second Firecracker sandbox from the same template (rapid provisioning)
6. Delete one sandbox manually
7. Watch the remaining sandbox get auto-reaped after its idle timeout expires

## Prerequisites

- qarax stack running: `./hack/run-local.sh`
- `qarax` CLI on PATH
- A host registered and initialized (run-local.sh does this automatically)
- The demo requires an initramfs that contains `qarax-init`, because it runs `qarax sandbox exec` inside the guest
- Firecracker is the default managed sandbox backend in this demo, and the exec step now runs against the Firecracker sandbox itself
- By default it uses `/var/lib/qarax/images/test-initramfs.gz` from the local e2e/demo environment; override with `--initramfs PATH` or `SANDBOX_INITRAMFS_PATH`
- The default run removes old `sandbox-demo-*` sandboxes, waits briefly for prior VM cleanup to settle, and refreshes its managed template assets before starting

## Usage

```bash
# Default: creates Firecracker + Cloud Hypervisor demo templates, benchmarks both, idle timeout 90s
./demos/sandbox/run.sh

# Custom server
./demos/sandbox/run.sh --server http://localhost:8000

# Reuse an existing template that boots with an exec-capable initramfs
./demos/sandbox/run.sh --template my-template

# Shorter idle timeout to see auto-reap faster
./demos/sandbox/run.sh --idle-timeout 30

# Remove demo-managed resources only
./demos/sandbox/run.sh --cleanup
```

## Options

| Flag | Default | Description |
|------|---------|-------------|
| `--server URL` | `$QARAX_SERVER` or `http://localhost:8000` | qarax API URL |
| `--template NAME` | `sandbox-demo-template` | VM template name; custom templates are reused and not deleted by the demo. When this is used, the built-in Firecracker vs Cloud Hypervisor benchmark is skipped. |
| `--idle-timeout SECS` | `90` | Idle timeout before auto-reap |
| `--initramfs PATH` | `/var/lib/qarax/images/test-initramfs.gz` | Initramfs path on the qarax-node host; must contain `qarax-init` |
| `--cleanup` | n/a | Remove demo-managed sandboxes, VMs, and template assets, then exit |

## How auto-reap works

The `sandbox_reaper` background task runs every 15 seconds and queries for
sandboxes where `last_activity_at + idle_timeout_secs < NOW()`.  Any matching
sandbox transitions to `destroying` and its VM is stopped and deleted.

Fetching a sandbox via `qarax sandbox get <id>` bumps `last_activity_at`,
resetting the idle clock — useful for keeping a sandbox alive while in use.

## Benchmark notes

The reported Firecracker vs Cloud Hypervisor numbers are simple end-to-end demo measurements from `sandbox create` until the sandbox reaches `ready`. They are useful for comparing this environment, but they are not a rigorous microbenchmark.

## Cleanup

The demo cleans up after itself (sandboxes + template) on exit.
If you interrupt early:

```bash
qarax sandbox list
qarax sandbox delete <id>
```
