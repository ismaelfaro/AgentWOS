# Committing and publishing

## Commit

Only when asked.

1. `git status -sb` and `git branch --show-current`. Confirm the branch is the
   intended one.
2. `git diff --stat`. Separate your changes from ones other sessions left.
3. Stage by name: `git add <file> ...`. Never `git add -A` or `git add .`.
4. Scan what is staged for secrets and personal data:
   `git diff --cached | grep -iE "sk-or-|api[_-]?key *=|@gmail\."`. A placeholder
   such as `sk-or-...` in documentation is fine; a real value is not.
5. Commit with a noreply identity through environment variables, leaving git
   config alone:

   ```bash
   N="<id>+<login>@users.noreply.github.com"
   GIT_AUTHOR_NAME=<login> GIT_AUTHOR_EMAIL=$N \
   GIT_COMMITTER_NAME=<login> GIT_COMMITTER_EMAIL=$N \
   git commit -m "..."
   ```

   `gh api user -q '"\(.id)+\(.login)@users.noreply.github.com"'` prints the
   address for the signed-in account.
6. Message: `type(scope): summary` in English, then what changed and why. End
   with the attribution line the session specifies.

## Never commit

- `Docs/`, `examples/*/web/samples/` (personal recordings), scratch folders,
  `.DS_Store`.
- Generated output: `target/`, `dist/`, `gen/`, `vendor/`, `runs/`,
  `node_modules/`.
- Anything left in the tree by another session.

## Push

Only when asked. `git fetch` first. If the remote has moved, rebase or merge;
do not force-push `main`. After pushing, report the commit, the branch, and what
was left uncommitted and why.

## Deploy

`examples/tab-jam/deploy.sh` publishes the live Tab Jam. Run it only when asked,
and check the live page afterwards.
