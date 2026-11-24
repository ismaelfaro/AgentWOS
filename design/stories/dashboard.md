# Stories: the dashboard

[Index](../README.md) · [Concept](../dashboard.md) · [Build](../build/dashboard.md)

Invented scenarios, using what the first version shows.

## The test that never ended

Pablo asks the coding harness to run his test suite and goes for coffee. When
he is back, the terminal shows a spinner and nothing else. He opens the
dashboard.

The graph has four nodes: him, the router, `coder`, `python`. The line from
`coder` to `python` is moving, and the `python` card says *running ×1* with the
last line `run -c "import time; time.sleep(300)"` (the test, in this story, is
a sleeping program). Under the cards a red line: *python: busy for 72s without
answering*.

He did not have to read a log. The agent that was waiting was the last one in
the chain, and the dashboard said which one.

## Everything is fine except one agent

A team runs six agents all day. The totals say 214 calls, 3 failures. The
failures are not spread out: the card for `search` says 3 of 11, and the alert
says so. The other five are clean.

The failures are programs that exited with an error, not calls that failed.
`search` answered every time, with *no matches*. A person reading the replies
would have thought it worked. The dashboard counts what the agent did, not only
what it said.

## An outside caller

Lucía serves two agents to a partner over HTTP and starts the dashboard next to
it. The graph has a dashed node, `outside`, at the top. Its line goes to
`reviewer`, and a second line runs on from `reviewer` to `git`.

On Tuesday the partner's calls triple. The count on the `reviewer` card climbs
and the line from `outside` stays lit. The dashboard cannot stop the calls (it
only looks), but Lucía sees it in the minute it happens and lowers
`calls_per_minute` in the manifest. The guard enforces the limit; the dashboard
is how she knew to change it.

## The browser tab someone else opened

A page on another site tries to read the dashboard through a name that points
to `127.0.0.1`. The request arrives with the other site's name as its `Host`,
and the server answers 403 before anything runs. Without the token in the URL
fragment, the data would have been refused anyway.
