# Agentic Tab Jam

A band of WASM agents that you conduct in plain English. Everything runs in one
browser tab: the agents, the OS control, the music and a 35 MB language model.
There is no server after the first load.

```
you: "drums play a breakbeat and make the melody dark"
        │
        ▼  kernel worker (web/kernel)
  ┌───────────┐   ┌───────────┐   ┌───────────┐
  │ Drummer   │   │ Bassist   │   │ Lead      │   one actor.wasm instance each,
  │ actor.wasm│   │ actor.wasm│   │ actor.wasm│   each with only its own tools
  └─────┬─────┘   └─────┬─────┘   └─────┬─────┘
        └── tool-llm ───┴── Needle (cactus.wasm) ──┘
        │ tool calls
        ▼
  director-agentWOS.wasm   allow | modify | block
        │
        ▼
  stage.wasm            band state -> notes for each bar -> WebAudio
```

## How it works

- **Needle** ([Cactus Compute](https://cactuscompute.com/needle), Apache-2.0) is a
  small tool-calling model. It maps one explicit instruction to one tool call. It
  does not chat and it does not improvise.
- **Each musician is an agent** with its own tool scope. The `actor` component
  splits a cue into clauses, keeps the ones that name its instrument, asks
  Needle for a tool call for each, and drops optional arguments the cue never
  mentioned.
- **The director, `director-agentWOS`, is an OS control.** It clamps the tempo to 60-160 bpm,
  moves a bass root that is outside C minor to the nearest note of the key,
  blocks a frantic lead at 150 bpm or more, and refuses to silence the whole band.
- **The stage** owns the band state and renders each bar as notes. The page plays
  them with WebAudio.
- **The audit log** shows every cue, pad gesture, tool call, confidence and verdict.

## Source layout

The stage has one Rust file per agent, all with the same functions
(`describe`, `brief`, `pad`, `apply`, `state`, `render`):

- `stage/src/lib.rs`: routes the `stage` interface to the agents.
- `stage/src/drummer.rs`: drum pattern and the band's tempo.
- `stage/src/bassist.rs`: bass line and the root everyone tunes to.
- `stage/src/lead.rs`: melody mood, randomness and synth voice.
- `stage/src/orchestra.rs`: sections added one by one.
- `stage/src/music.rs`: notes, scales and helpers they share.

The other pieces: `controls/director-agentWOS` (the OS control), `web/src`
(page, pads, signal spikes, synthesizer and kernel worker) and
`web/test/smoke.mjs`.

## Touch pads

Each agent card has an XY pad for its two main settings:

| Agent | Across | Up |
|---|---|---|
| Drummer | style | tempo (bpm) |
| Bassist | root note | movement |
| Lead | mood | randomness (0 repeats one phrase, 100 changes every note) |

Drag on a pad, or focus it and use the arrow keys. The crosshair shows the
values the band is using. A dashed ring shows where you asked for; the two
differ when the director changed or blocked the call, for example when
you drag the tempo above 160.

The Lead card also has five sounds to choose from: `saw`, `supersaw`, `piano`,
`square` and `bell`. A cue such as `Lead sound bell` picks one too. With a local
recording loaded (see below), `sample` appears as a sixth option.

A pad sends the tool call straight to the kernel, without asking the model, so
it responds while you drag. It still goes through the control chain and the
audit log, and a text cue moves the crosshair too. The pads are described by the
stage (`pads` in `setup`), so the page has no band-specific pad code.

## The Orchestra Director Agent

The director (`director-agentWOS`) has its own card at the top of the band, "Orchestra
Director Agent". It shows the rules it enforces, how many calls it has allowed,
changed, blocked and flagged, and its latest verdict.

The rules on the card come from the control itself: the `control` interface has
a `describe` function, and the director builds its answer from the same
constants it uses to inspect calls.

## Signals between agents

Streaks of light travel between the cards when one agent's setting reaches
another:

- **Tempo and bar clock**, from the Drummer to everyone: faint on every
  downbeat, bright when the tempo changes.
- **Root note**, from the Bassist to the Lead and every orchestra section,
  when the root changes and they retune.
- **Your cue**, from the text box to each agent that took it.
- **Inspection**, from an agent to the Orchestra Director Agent and back for every
  tool call: the return spike is green when allowed, amber when changed and
  red when blocked.

The stage lists these links in its state (`links`: who follows whom and which
field carries the signal), so a section you add is wired in as soon as it
joins. Spikes are off when the system asks for reduced motion.

## Building an orchestra

The last tile in the band, **+ Add agent**, adds one orchestra section at a
time: Strings, Brass, Flute, Choir, Organ, then Strings 2 and so on. Each one is
a new `actor.wasm` instance, created while the band plays.

- **Each section is different.** It gets its own timbre, register, chord note
  (root, third, fifth, seventh or octave), detune and pulse, and its own pad
  (detune across, pulse up) and note buttons.
- **All of them stay together.** Every section tunes to the bass root, so
  changing the bass moves the whole orchestra, and each pulse is a division of
  the bar, so they all breathe on the band's clock.
- **The OS decides who gets in.** The request to add an agent goes through the
  director like any tool call. It allows eight sections and blocks the
  ninth, and it clamps a section's detune to 35 cents.
- **Level is shared.** Sounding sections split one level budget, so a full
  orchestra is not louder than a single section.

Text cues reach a section by its name: `strings hold the seventh`,
`brass detune 20`, `brass pulse quarter`.

The idea comes from massed-oscillator drone instruments, where many simple
voices, each tuned a little differently, add up to one large sound.

## Run

Requires Rust with `wasm32-wasip2` and Node.js.

```bash
examples/tab-jam/build.sh
tools/serve.py
# open http://localhost:8765/examples/tab-jam/web/dist/index.html
```

`build.sh` builds the components, downloads the Cactus engine (pinned to v4.0.0
and checksum-verified), transpiles the components with `jco` and bundles the page.
The model is downloaded by the browser from Hugging Face on first start and cached.

Test the kernel with the real model in Node:

```bash
(cd examples/tab-jam/web && node test/smoke.mjs)
```

## Sampled lead (optional)

The lead can play a recording instead of the synthesized voice. Put two files in
`web/samples/` and rebuild:

- `lead.wav`: a short mono recording that loops cleanly.
- `lead.json`: `{ "file": "lead.wav", "midi": 60 }`, where `midi` is the pitch the
  recording sounds at.

Each note re-pitches the recording. `web/samples/` is git-ignored, so recordings
stay on your machine; without them the synthesized lead plays. When a sample is
loaded, `sample` appears among the lead's sound buttons.

## Cues that work

Name the instrument and say what you want:

- Drums: `Drums double time`, `Make the drums jazz`, `Make the drums random`,
  `Drums play a breakbeat`, `Silence the drums`
- Bass: `Bass walking on F`, `Funky bass on D`, `Bass octaves on G`; `Bass on A`
  shows the director moving the root into the key
- Lead: `Make the melody dark`, `Lead goes jazz`, `Lead goes frantic`
- Tempo: `Set the tempo to 140`, `speed to 120`; `Set the tempo to 300` shows the
  director clamping it
- Two at once: `Drums play a breakbeat and make the melody bright`

Each agent card lists what that agent understands, and an ignored cue shows the
same list in the audit log. The suggestion buttons come from the stage, and the
smoke test checks that every one of them works.

## Limits

- Needle needs explicit wording. Idioms such as "drop the drums", "faster"
  without a number or "drum 120" are ignored. The audit log shows the confidence.
- Needle is sensitive to how tools are declared, including the order of JSON
  keys. The components build with `serde_json`'s `preserve_order` for that reason.
- A clause that names no instrument reaches no agent.
- For orchestra sections the model handles the note, the detune and the
  `quarter` and `eighth` pulses. It ignores `sixteenth`, and it misreads
  `pulse half` as a detune. Use the pad for the pulse.
- A section cannot be removed once added; set it to `silent` instead.
- The music is generated by rules from the band state. The model picks the
  settings; it does not write notes.
- The first start needs a network connection to fetch the model.
