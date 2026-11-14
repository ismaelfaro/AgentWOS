# Manifest

[Index](README.md) · [OS model](os-model.md) · [Protocol](protocol.md) · [Distribution](distribution.md) · [Manifest](manifest.md) · [Roadmap](roadmap.md)

The manifest is the init table of the system: what to load, how the parts are
named, and what each may do. One shape for every mode: a list of resources,
each with `kind`, `metadata` and `spec`. Paths resolve from the manifest's
folder, as they do today.

Minimal kinds:

| Kind | Declares |
|---|---|
| `Agent` | A component, its labels, and what its imports are wired to |
| `Service` | A driver: a component given a narrow way to the outside (LLM, tool), a kernel service (starting programs), or an external host |
| `Policy` | Selector, ingress, egress, delegation, limits |
| `Control` | A control component |
| `Run` | What to start: the entry agent or scenario, and run options |
| `Realm` | A peer kernel: its key, endpoint and trust labels |
| `Pool` | A named, changing set of agents and how calls to it are dispatched |

The CLI harness example, rewritten:

```json
{
  "apiVersion": "agentwos/v1",
  "resources": [
    { "kind": "Service", "metadata": { "name": "llm" },
      "spec": { "type": "llm", "module": "../../modules/llm-call/target/wasm32-wasip2/release/llm_call.wasm",
                "model": "openrouter/nvidia/nemotron-3-ultra-550b-a55b:free" } },

    { "kind": "Service", "metadata": { "name": "openrouter.ai" }, "spec": { "type": "host" } },

    { "kind": "Service", "metadata": { "name": "workspace" },
      "spec": { "type": "workspace", "root": "runs/workspace",
                "programs": ["python3", "node", "cargo", "rustc", "pytest"], "timeoutSeconds": 30 } },

    { "kind": "Agent", "metadata": { "name": "router", "labels": { "role": "router" } },
      "spec": { "module": "router/target/wasm32-wasip2/release/cli_router.wasm" } },

    { "kind": "Agent", "metadata": { "name": "ask", "labels": { "role": "worker" } },
      "spec": { "module": "plugins/ask/target/wasm32-wasip2/release/plugin_ask.wasm",
                "description": "Answers a question with a language model." } },

    { "kind": "Agent", "metadata": { "name": "coder", "labels": { "role": "worker", "writes": "code" } },
      "spec": { "module": "plugins/coder/target/wasm32-wasip2/release/plugin_coder.wasm",
                "description": "Given a task, writes code in the workspace, runs it and fixes it until it works." } },

    { "kind": "Policy", "metadata": { "name": "llm-egress" },
      "spec": { "selector": { "name": "llm" },
                "ingress": [{ "from": {}, "verbs": ["llm.complete"] }],
                "egress":  [{ "to": { "name": "openrouter.ai" }, "verbs": ["http.request"] }] } },

    { "kind": "Policy", "metadata": { "name": "router" },
      "spec": { "selector": { "name": "router" },
                "ingress": [{ "from": { "name": "user" }, "verbs": ["agent.run"] }],
                "egress":  [{ "to": { "name": "llm" }, "verbs": ["llm.complete"] },
                            { "to": { "labels": { "role": "worker" } }, "verbs": ["agent.run"] }],
                "delegation": { "to": { "labels": { "role": "worker" } }, "lend": [], "maxDepth": 1 } } },

    { "kind": "Policy", "metadata": { "name": "workers" },
      "spec": { "selector": { "labels": { "role": "worker" } },
                "ingress": [{ "from": { "name": "router" }, "verbs": ["agent.run"] }],
                "egress":  [{ "to": { "name": "llm" }, "verbs": ["llm.complete"] }],
                "limits":  { "calls": 40, "seconds": 120 } } },

    { "kind": "Policy", "metadata": { "name": "code-writers" },
      "spec": { "selector": { "labels": { "writes": "code" } },
                "egress": [{ "to": { "name": "workspace" }, "verbs": ["workspace.*"],
                             "where": { "program": ["python3", "pytest"] } }] } },

    { "kind": "Run", "metadata": { "name": "cli" },
      "spec": { "entry": "router", "default": "coder" } }
  ]
}
```

What this says that the current manifest cannot:

- The LLM service may reach `openrouter.ai` and nowhere else.
- Only the router may call workers; workers cannot call each other.
- `coder` may run `python3` and `pytest`, not `cargo`, although the workspace
  allows it for someone else.
- A new worker with `"role": "worker"` gets the worker policy without any
  policy edit.
- The router lends nothing (`"lend": []`): a worker acts with its own authority.

Migration is mechanical. `"grants": ["llm"]` becomes one egress rule; a manifest
with no `Policy` resources and old-style `grants` can be translated on load.
Scenario manifests map the same way: `scenario` becomes the `Run` entry,
`controls` become `Control` resources.

## How it stays minimal but extensible

| Extension point | How | Without changing |
|---|---|---|
| New party types | A new `Service.spec.type` | Policy, envelope |
| New resource kinds | A new `kind`; unknown kinds are an error, not ignored | Existing kinds |
| New constraints | A new key in a rule's `where` | Rule structure |
| New verbs | Follow from new WIT interfaces | Policy syntax |
| New limits | A new key in `limits` | Delegation rules |
| Programmable checks | Controls | The kernel |
| Schema changes | `apiVersion` | Old manifests |

Two rules keep extension safe:

- **Fail closed.** An unknown `kind`, an unknown `where` key or an unknown limit
  is a load error. A typo in a policy must not silently become "allowed".
- **The core never interprets `body`.** Only `where` constraints and controls
  look inside it, so new interfaces need no kernel change beyond naming the verb.
