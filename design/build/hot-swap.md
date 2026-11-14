# Building hot swapping

[Index](../README.md) · [Concept](../hot-swap.md) · [Stories](../stories/hot-swap.md)

How to implement revisions, the ways of swapping, state hand-over and workflow
resume. It assumes `Kernel::dispatch` and the ledger from
[Building the protocol](protocol.md). Code blocks are sketches; they have not
been compiled.

## Revisions

```rust
// host/src/registry.rs
pub struct Revision {
    pub digest: String,              // sha256 of the component bytes
    pub settings: serde_json::Value, // prompt, preferred model, thresholds
    pub serves: Vec<VerbSpec>,       // from handler.describe()
    pub resumes: Vec<String>,        // digests whose running trees it can take over
}

pub struct Name {
    pub current: usize,              // index into `revisions`
    pub revisions: Vec<Revision>,    // the archive; nothing is deleted
    pub live: Vec<usize>,            // revisions still serving some tree
}
```

Store this on disk as one JSON file per name plus a `components/<digest>.wasm`
cache. A swap writes the file with `current` changed, by write-then-rename, the
way `runner.rs` already writes its log.

**Settings have to reach the agent.** Today a prompt is a string inside the
component, so changing it means recompiling. Add `kernel.settings() -> string`
to the kernel interface, and move prompts and thresholds into `settings`. This
is the change that makes the cheapest kind of improvement possible.

## The configuration of a tree

```rust
pub struct Tree {
    pub root: IntentId,
    pub pinned: HashMap<String, usize>,   // name -> revision index, taken at start
}
```

`dispatch` looks up the tree of the intent (through `parent`). A root intent
creates a tree and copies `current` for every name into `pinned`. A child intent
uses its tree's `pinned`. That is the whole of version consistency.

Each `Name` counts the trees pinned to each revision. When the count of a
non-current revision reaches zero it leaves `live`, and its instance is dropped.

## Instances

Keep a cache of instances keyed by `(name, digest)`. Two choices:

| Choice | Effect |
|---|---|
| Reuse an instance across intents | Fast. The agent can keep state in its memory, which a swap then loses |
| A fresh instance per root intent | Statelessness becomes a fact, not a convention. Costs an instantiation per tree |

Start with a fresh instance per root intent. Compiling is the slow part and is
cached per digest; instantiating a compiled component is cheap by comparison.
Measure before optimising.

## The ways of swapping

| Way | Implementation |
|---|---|
| `next` | Change `current`. Nothing else |
| `drain` | `next`, plus a deadline on the old revision; at the deadline, cancel its trees by epoch interruption and return their budgets |
| `canary` | At root-intent time, pin the candidate for a share of trees. Choose by hashing the root id, so a retry of the same tree gets the same revision |
| `shadow` | After delivering to the current revision, deliver a copy to the candidate in a tree marked `dry_run`. In a dry-run tree, tool agents answer from the ledger's recorded results or refuse; results are written to the ledger and returned to nobody |
| `migrate` | Ask the old instance `state.export`, instantiate the new one, ask it `state.import`. If import fails, keep the old one |
| `resume` | See below |

## Resuming a workflow

A workflow is the tree under one root intent. Its finished steps are child
intents already in the ledger with their results.

1. The new revision must list the old digest in `resumes`.
2. The kernel pins the new revision for that tree and starts a new instance.
3. It delivers the original root intent again, in a tree marked `replay`.
4. In a replay tree, when the agent makes a child intent that matches one in the
   ledger (same verb, same body), `dispatch` returns the recorded result without
   delivering it. The first child intent with no match ends replay, and the tree
   continues normally.

Step 4 only works if the new revision makes the same early calls as the old one.
Test that before allowing it: replay several *finished* trees from the ledger
through the new revision and require that it reaches the end with no unmatched
call before the point where the old one stopped.

## Compatibility

On `registry.propose`, compare `serves` of the candidate with the current
revision:

- Every current verb present, and each input schema accepts what the old one
  accepted: mark the candidate `compatible`.
- Otherwise: mark which verbs changed, and find callers of those verbs in the
  ledger. The Registry refuses a global `next` and offers a per-caller swap.

## Limits on live revisions

`live.len()` is capped (start with 3). Adding a revision past the cap puts a
deadline on the oldest, as in `drain`.

## The browser kernel

`web/kernel/kernel.js` instantiates each agent from a transpiled component. A
swap there is: load the new component's core modules, instantiate, and replace
the entry in the `actors` array at the next cue. Tab Jam already rebuilds agents
at runtime in `spawn`; the same path serves a swap. The build puts each
component under a folder named by build id, which already avoids pairing new
glue with old modules.

## Order

1. `kernel.settings`, and move one agent's prompt into settings.
2. Registry with `next` and rollback for settings only.
3. Trees and pinning.
4. Component revisions, cache by digest, fresh instance per tree.
5. `canary`, with the revision digest in every ledger entry.
6. `shadow` and dry-run trees.
7. `drain` and the cap.
8. `migrate`, then `resume`.

## How to know it works

| Check | Kind |
|---|---|
| A tree started before a swap finishes on the old revision; one started after uses the new one | Integration test with a slow agent |
| Rolling back restores byte-identical behaviour | Replay test |
| A canary share of 0.1 sends about a tenth of trees to the candidate, and a retried tree keeps its revision | Statistical and unit test |
| A shadow tree writes nothing through a tool | Integration test with a files tool |
| A revision that cannot import the old state is rejected for `migrate` and the old one keeps running | Integration test |
| `resume` skips recorded steps and runs the remaining ones exactly once | Integration test counting tool calls |
