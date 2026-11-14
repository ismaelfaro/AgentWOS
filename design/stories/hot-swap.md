# Stories: hot swapping

[Index](../README.md) · [Concept](../hot-swap.md) · [Build](../build/hot-swap.md)

Invented scenarios that show revisions, the ways of swapping, state hand-over
and resuming a workflow. The first one uses the band from the
[Tab Jam example](../../examples/tab-jam/README.md). The situations are made up;
the mechanics are the ones in the design.

## Changing the drummer mid-song

The band is playing. Agent Drummer, revision `a3f1`, is holding a breakbeat at
140. The Proposer has a new revision, `9bd2`, that understands "swing it" and
"play behind the beat". Nobody wants to stop the song to find out whether it is
any good.

**Shadow.** For the next thirty-two bars, `9bd2` sits at a second kit that makes
no sound. It gets a copy of every cue the real drummer gets. Its answers go to
the ledger and nowhere else.

```json
{ "intent": "e-7714", "to": "drummer", "revision": "a3f1", "result": "drums: breakbeat" }
{ "intent": "e-7714-shadow", "to": "drummer", "revision": "9bd2", "dry_run": true, "result": "drums: breakbeat" }
```

Thirty-one of thirty-two match. On the one that differs, the cue was "swing it":
the old revision ignored it, the new one switched to jazz.

**Canary.** One bar in ten now goes to `9bd2` for real. In this band one bar is
one intent tree, and a tree runs on one configuration, so a bar is played
entirely by one drummer or entirely by the other. No bar gets half of each.

```
bar 41  a3f1     bar 42  a3f1     bar 43  9bd2     bar 44  a3f1
```

**Hand-over.** The new drummer needs to know the tempo and the pattern in play.
Agent Drummer serves `state.export`:

```json
{ "schema": 2, "tempo": 140, "style": "breakbeat", "bar": 43 }
```

and `9bd2` serves `state.import`. It understands schema 2 and starts on bar 43,
at 140, on the breakbeat. Had it understood only schema 3, the import would have
failed and the old drummer would have played on. A swap that cannot carry the
state does not happen; it is never attempted halfway.

**Next.** After fifty canary bars with no complaints from the sound engineer,
the Registry moves the name. From the next bar, every bar is `9bd2`. The old
drummer finishes the bar it is in and is not asked again.

**Rollback.** Twenty bars later the conductor says "silence the drums", and the
new revision, it turns out, has opinions about silence. Guard blocks on Agent
Drummer go from none to four in a minute. The Registry sees that in the ledger
and moves the name back to `a3f1`, on the next bar, without being asked. The
ledger entry says why. The audience hears a fill that lands slightly early, and
then the old drummer, exactly as before, because `a3f1` was never changed.

| What happened | Component |
|---|---|
| The candidate played silently next to the real one | `shadow`, dry-run tree |
| One bar in ten, and never half a bar | `canary`; one configuration per intent tree |
| Tempo and pattern were handed across | `state.export`, `state.import`, schema check |
| The switch happened between bars | `next`; the moment between intents is the swap point |
| It went back by itself | Automatic rollback on ledger signals |

## The eleven-day report

Agent Cronista writes the annual report. It is a workflow of forty steps across
eleven days: gather figures from six departments, chase the two that are late,
draft, send for review, wait, revise. On day seven, with twenty-six steps done,
a better Cronista is promoted. It writes clearer summaries.

Starting over would mean asking six departments for their figures again. Three
of them have gone on holiday.

The new revision declares that it can take over from the old one:

```json
{ "digest": "sha256:c41a...", "resumes": ["sha256:88e0..."] }
```

Before trusting that, the Evaluator replays last year's finished report through
the new revision. It makes the same first forty requests in the same order, so
the claim holds.

The kernel then starts the new Cronista on this year's root intent, in replay.
It asks finance for the figures. The ledger already has that intent and its
answer, so the answer is handed back at once and finance is not disturbed. The
same happens twenty-five more times, in under a second. On the twenty-seventh
step there is nothing in the ledger. Replay ends, and the new Cronista sends a
real request, the first new work of its life, on day seven of a workflow it
did not start.

The departments on holiday never notice. The summaries, from page forty onwards,
are noticeably clearer.

## Three is a crowd

The translation agent has been improved four times this week, and the overnight
batch job started on Monday is still pinned to Monday's revision. So is
Tuesday's long job to Tuesday's. A fifth revision arrives.

A name may have three revisions alive. The Registry puts a deadline on Monday's:
two hours. Monday's batch job has four hours of work left.

At the deadline the job is cancelled. Its unused budget goes back to whoever
lent it, the ledger records the cancellation and the reason, and the person who
owns the job gets an intent in their queue saying so. The job restarts on the
current revision.

It is not a kind outcome for the Monday job. It is a stated one: a system that
keeps every old revision alive for ever ends up running all of its past at once.
