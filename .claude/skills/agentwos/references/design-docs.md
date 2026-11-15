# Designing a feature

A feature design is three documents, written together.

| Document | Path | Answers |
|---|---|---|
| Concept | `design/<feature>.md` | What it is, why, how it fits the rest, what can go wrong, open questions |
| Build notes | `design/build/<feature>.md` | Which files and components change, in what order, and how to know it works |
| Stories | `design/stories/<feature>.md` | Invented, concrete and enjoyable scenarios that show the mechanics, with the intents or log lines the system would produce |

## Steps

1. Read `design/README.md` for the narrative and the vocabulary, and
   `design/architecture.md` for the rule that everything that runs is an agent.
2. Read the code the feature would touch. Build notes must name real files and
   functions.
3. If asked to research, search the web and link sources. Mark anything stated
   from memory as not re-checked.
4. Write the concept. Start it with a status line that says what is built and
   what is not.
5. Write the build notes: components to add, a smallest first slice, and a table
   of checks. Mark code blocks as sketches unless they were compiled.
6. Write the stories. Make them specific and a little playful, and end each with
   a table mapping what happened to the component responsible. Include at least
   one story where the feature says no.
7. Link them: the navigation line at the top of each, a "See also" line under it
   in the concept, a row in the feature table of `design/README.md`, and a line
   in the root `README.md` design list. Copy the format from an existing feature.
8. Validate: every relative link resolves, every `json` block parses, any WIT
   sketch passes `wasm-tools component wit`.

## Framing

- AgentWOS is its own kind of system: AI first, cloud native, local first,
  self-improving. Lead with its own terms. Mention a classic operating system or
  Kubernetes relative only as an aid.
- The unit that runs is the agent. Models, tools, guards and gateways are agents
  with a narrower job.
- Keep it minimal and extensible: the fewest components, and a stated way to add
  more.
- State limits plainly, especially where safety depends on a model behaving.
