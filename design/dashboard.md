# Dashboard: an agent that watches the other agents

[Index](README.md) · [Persistence](persistence.md) · [Architecture](architecture.md) · [Protocol](protocol.md)

See also: [how it is built](build/dashboard.md) · [stories](stories/dashboard.md)

Status: built, first version. Where a section says "next", it is not.

## The idea

The kernel mediates every call, so it already knows everything the agents do.
Showing it needs no help from the agents and no change to them. The dashboard
is an agent like the rest: a WebAssembly component. What makes it a monitor is
the one thing it is granted, `observe`, a read-only window on the kernel's
record.

```
agents ──calls──► kernel ──writes──► ledger ◄──reads── dashboard agent ──► page
                  (mediates)                (observe, read-only)
```

## Two ways to run it

**On its own.** `agentwos-host --dashboard 127.0.0.1:8830` starts only the
dashboard. It finds whatever kernel is running, watches it, and when that
kernel ends it waits for the next one and connects to it. It can be started
before or after the kernel, and left running all day, the way a process
monitor is.

**Inside a kernel.** `--harness ... --dashboard ADDR` runs the same component in
the same process, reading the ledger directly.

How a kernel is found: every running harness serves its ledger, read-only, on a
loopback port the system picks, and leaves a **note** with the port and a token
in a folder only this user can open (`$TMPDIR/agentwos-<uid>/kernels/<pid>.json`,
mode 600). The note goes when the kernel ends; a monitor deletes the note of a
kernel that is gone. `--no-observe` keeps a kernel out of sight. `--kernel PID`
makes a monitor watch one kernel instead of the newest.

```
kernel (harness)                         monitor (dashboard, its own process)
  ledger ──► /observe/* on 127.0.0.1:<any>  ◄── reads, with the token from the note
         └─► note: <run dir>/kernels/<pid>.json  ◄── finds it
```

The window has no route that writes: `GET` only. The dashboard component does
not know which way it runs; the host gives it the same `observe` import either
way, plus `source` to say what it watches.

## Who does what

| Part | Job | Knows nothing about |
|---|---|---|
| Kernel | Keeps the ledger: one event per call, model call, file and program; per-agent counters; who asked whom. Bounded to the latest few thousand events | What counts as trouble, how anything looks |
| Dashboard agent | Decides what is an alert (stuck, failing), builds the JSON and the page, writes a text report | Where events come from, how the page is served |
| Server | Carries the page and the JSON to a loopback address | What is in them |

The split matters. The rule "busy for more than 60 seconds is stuck" is the
agent's, so it can be changed, replaced or improved without touching the kernel.

## What it shows

- Totals: agents, running now, calls, failures, model calls, files, programs.
- One card per agent: running or idle or in alert, its counters, how long its
  calls take, what it did last, and who asks it.
- A graph of who asks whom, layered from whoever asks first (you, the router, a
  caller on the network). A call in progress is a moving line.
- Alerts: busy without answering for too long; half or more of the work failed.
- A live feed of events. Click an agent to see only its events and the calls
  it made.
- A text report for a terminal.

A failure counts calls that returned an error and also what an agent did that
failed: a program that exited with an error, a model call that failed. A
program can fail while the agent that ran it answers perfectly well.

## Safety

- It listens on the loopback address only, and only on that.
- The page holds no data and needs no token. Everything else needs a token the
  host makes at start and prints with the link. The token is in the URL
  fragment, so it never goes in a request line.
- A request whose `Host` is not a loopback name is refused, so a web page on
  another site cannot use the dashboard through DNS tricks.
- The agent has no network, files or programs, and no other agent. It sees
  names, times, counts and a short line per event. It cannot change anything.

## Next

[Persistence](persistence.md) lists what the dashboard grows into: a process
view with timers, mailboxes and checkpoints; cost per result; skeleton versus
full traces; and controls (pause, drain, kill) as a separate grant.
