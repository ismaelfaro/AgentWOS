# Manifest

[Index](README.md) · [Architecture](architecture.md) · [Protocol](protocol.md) · [Distribution](distribution.md) · [Evolution](evolution.md) · [Hot swapping](hot-swap.md) · [Hive](hive.md) · [State of the art](state-of-the-art.md) · [Manifest](manifest.md) · [Roadmap](roadmap.md)

The manifest is the init table of the system: what to load, how the parts are
named, and what each may do. One shape for every mode: a list of resources,
each with `kind`, `metadata` and `spec`. Paths resolve from the manifest's
folder, as they do today.

Minimal kinds:

| Kind | Declares |
|---|---|
| `Agent` | A component to run, its labels, the verbs it serves and its settings |
| `Policy` | Selector, ingress, egress, mandate, limits, attached guards |
| `Run` | What to start: the entry agent, and run options |
| `Realm` | A peer kernel: its key, endpoint and trust labels |
| `Pool` | A named, changing set of agents and how calls to it are dispatched |

The CLI harness example, rewritten:

```json
{
  "apiVersion": "agentwos/v1",
  "resources": [
    { "kind": "Agent", "metadata": { "name": "model", "labels": { "role": "model", "runs": "remote" } },
      "spec": { "module": "../../modules/llm-call/target/wasm32-wasip2/release/llm_call.wasm",
                "serves": ["model.complete"],
                "settings": { "model": "openrouter/nvidia/nemotron-3-ultra-550b-a55b:free" } } },

    { "kind": "Agent", "metadata": { "name": "workspace", "labels": { "role": "tool", "runs": "local" } },
      "spec": { "builtin": "workspace",
                "serves": ["files.read", "files.write", "files.list", "program.run"],
                "settings": { "root": "runs/workspace", "timeoutSeconds": 30,
                              "programs": ["python3", "node", "cargo", "rustc", "pytest"] } } },

    { "kind": "Agent", "metadata": { "name": "router", "labels": { "role": "router" } },
      "spec": { "module": "router/target/wasm32-wasip2/release/cli_router.wasm" } },

    { "kind": "Agent", "metadata": { "name": "ask", "labels": { "role": "worker" } },
      "spec": { "module": "plugins/ask/target/wasm32-wasip2/release/plugin_ask.wasm",
                "description": "Answers a question with a language model." } },

    { "kind": "Agent", "metadata": { "name": "coder", "labels": { "role": "worker", "writes": "code" } },
      "spec": { "module": "plugins/coder/target/wasm32-wasip2/release/plugin_coder.wasm",
                "description": "Given a task, writes code in the workspace, runs it and fixes it until it works." } },

    { "kind": "Policy", "metadata": { "name": "model-egress" },
      "spec": { "selector": { "name": "model" },
                "ingress": [{ "from": {}, "verbs": ["model.complete"] }],
                "egress":  [{ "to": { "host": "openrouter.ai" }, "verbs": ["http.request"] }] } },

    { "kind": "Policy", "metadata": { "name": "router" },
      "spec": { "selector": { "name": "router" },
                "ingress": [{ "from": { "name": "user" }, "verbs": ["agent.run"] }],
                "egress":  [{ "to": { "labels": { "role": "model" } }, "verbs": ["model.complete"] },
                            { "to": { "labels": { "role": "worker" } }, "verbs": ["agent.run"] }],
                "mandate": { "to": { "labels": { "role": "worker" } }, "lend": [], "maxDepth": 1 } } },

    { "kind": "Policy", "metadata": { "name": "workers" },
      "spec": { "selector": { "labels": { "role": "worker" } },
                "ingress": [{ "from": { "name": "router" }, "verbs": ["agent.run"] }],
                "egress":  [{ "to": { "labels": { "role": "model" } }, "verbs": ["model.complete"] }],
                "limits":  { "calls": 40, "seconds": 120 } } },

    { "kind": "Policy", "metadata": { "name": "code-writers" },
      "spec": { "selector": { "labels": { "writes": "code" } },
                "egress": [{ "to": { "name": "workspace" }, "verbs": ["files.*", "program.run"],
                             "where": { "program": ["python3", "pytest"] } }] } },

    { "kind": "Run", "metadata": { "name": "cli" },
      "spec": { "entry": "router", "default": "coder" } }
  ]
}
```

What this says that the current manifest cannot:

- The `model` agent may reach `openrouter.ai` and nowhere else.
- Workers reach "a model", by label. Adding a local model agent
  (`"runs": "local"`) gives them a second one with no policy edit.
- Only the router may call workers; workers cannot call each other.
- `coder` may run `python3` and `pytest`, not `cargo`, although the workspace
  allows it for someone else.
- A new worker with `"role": "worker"` gets the worker policy without any
  policy edit.
- The router lends nothing (`"lend": []`): a worker acts with its own authority.

Migration is mechanical. `"grants": ["llm"]` becomes one egress rule; a manifest
with no `Policy` resources and old-style `grants` can be translated on load.
Scenario manifests map the same way: `scenario` becomes the `Run` entry, and
each guard becomes an `Agent` that serves `guard.inspect`, attached to a
policy.

There are three kinds for one kernel and two more for distribution. Models,
tools and guards are all `Agent` resources; nothing else runs. `"builtin": "workspace"` marks the one agent the kernel still provides
itself, until it is a component ([Roadmap](roadmap.md)).

## How it stays minimal but extensible

| Extension point | How | Without changing |
|---|---|---|
| New kinds of agent | New verbs in `serves` | Kinds, policy, intent |
| New resource kinds | A new `kind`; unknown kinds are an error, not ignored | Existing kinds |
| New constraints | A new key in a rule's `where` | Rule structure |
| New verbs | Follow from new WIT interfaces | Policy syntax |
| New limits | A new key in `limits` | Mandate rules |
| Programmable checks | Guards | The kernel |
| Schema changes | `apiVersion` | Old manifests |

Two rules keep extension safe:

- **Fail closed.** An unknown `kind`, an unknown `where` key or an unknown limit
  is a load error. A typo in a policy must not silently become "allowed".
- **The core never interprets `body`.** Only `where` constraints and guards
  look inside it, so new interfaces need no kernel change beyond naming the verb.
