# Stories: distribution

[Index](../README.md) · [Concept](../distribution.md) · [Build](../build/distribution.md)

Invented scenarios that show realms, pools, swarms, local tools and moving a
component between kernels. The situations are made up; the mechanics are the
ones in the design.

## Grandmother's paella

Abuela Carmen's paella recipe lives in one text file on a laptop from 2015. The
family has tried to get it out of her for thirty years. Her granddaughter Lucía
wants to cook it for sixty people at a wedding, and she needs it scaled,
costed, and checked for the three guests who cannot eat shellfish.

Carmen's laptop runs a kernel. That makes it a realm, `abuela`, with a files
tool rooted at one folder. Lucía's realm, `lucia`, is admitted by one policy.
Nothing else on the internet is.

Lucía asks a **pool** of experts in a cloud realm, `mercado`:

```json
{ "kind": "Pool", "metadata": { "name": "cocina" },
  "spec": { "selector": { "labels": { "expertise": "cooking" } },
            "realms": ["mercado"], "dispatch": "all" } }
```

Five members answer the adverts: one that scales quantities, one that prices
ingredients, one for allergens, one for timing, and one that only has opinions
about rice.

The recipe cannot leave the laptop. Carmen has been clear. So the pool members
are not sent the file. Lucía's kernel fetches each expert **as a component**, by
digest, and runs them on Carmen's laptop:

```
advert   escalador@mercado   digest sha256:7c1e...   offers: ["call", "module"]
fetched  7c1e...             verified                instantiated in realm abuela, no grants
```

Each expert gets one thing, lent for this task and narrowed first:

```
files.read   on   recetas/paella.txt      until this intent tree ends
```

The pricing expert needs current prices, so it holds a second grant, to ask a
price tool. It does not hold any way to send the recipe anywhere: an agent has no
network unless a rule gives it one, and no rule does.

Then the saffron. The scaling expert says 3 grams for sixty people. The timing
expert, asked the same question, says 30. The rice expert has a third opinion and
a paragraph about tradition. Lucía's kernel sends that one question to the pool
with `dispatch: quorum`, needing three matching answers out of five. Four say 3.

The wedding has paella. The recipe file was read five times, by five programs
that ran on Carmen's laptop and stopped existing when the task ended. Carmen's
ledger says exactly who read what. She checks it. She remains suspicious, which
is her right.

| What happened | Component |
|---|---|
| The laptop decided who could reach it | Realm, default deny between kernels |
| "A cooking expert", without knowing which | Pool, adverts |
| Experts ran next to the data | Component fetched by digest, run with no grants |
| One file, one task | Attenuated grant, lent in a mandate |
| Disagreement settled by counting | `dispatch: quorum` |
| Carmen could audit it | The ledger of her own realm |

## The lighthouse

The research station at the end of the fjord has a satellite link that works
when the weather feels like it. Its kernel runs three agents, a small model on a
local machine, and a files tool. It has not seen another realm in nine days.

Nothing is wrong. A kernel with a local model and local tools is a complete
system. The agents log tide readings, argue with each other about a suspicious
sensor, and write the daily report. Their intents go through the same policy and
the same ledger as anywhere else.

On the tenth day the link comes up for forty minutes. The station's gateway
agent has a queue. Adverts from the university realm arrive, and with them a
newer revision of the sensor-calibration expert. The kernel fetches it by
digest, checks the bytes, and holds it as a candidate. The link drops at minute
thirty-eight.

It does not matter that the link dropped. The component is on the station now.
It is evaluated against last week's readings, on the station, by the station's
own Evaluator. Distribution gave the lighthouse reach. It never became a
dependency.

## A swarm with a ceiling

Tomás asks his agent to find a flat in Valencia. The agent decides this is a job
for a swarm and asks the `inmobiliaria` pool with `dispatch: all`. Each member,
being thorough, decides its part is also a job for a swarm.

Unbounded, this is how a search for a two-bedroom flat becomes ten thousand
model calls. Bounded, it is arithmetic done before anything starts:

```json
"mandate": { "to": { "labels": { "role": "search" } }, "maxFanout": 4, "maxDepth": 2 }
```

```
root budget          200 calls
depth 1, 4 members   50 calls each
depth 2, 4 each      12 calls each
depth 3              refused: maxDepth
worst case           4 + 16 = 20 agents, 200 calls, known in advance
```

One member finds the flat on its ninth call. Tomás's agent stops waiting. The
cancellation travels down the tree, the other members are stopped mid-search,
and their unused calls go back up the chain. The ledger records 61 calls spent
and 139 returned.

The member that wanted a third level of swarm has a denial in the ledger and,
one assumes, a sense of unfinished business.
