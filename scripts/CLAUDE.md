# Fork Maintenance

This branch is a fork of `astral-sh/ruff` that widens visibility in
`ty_python_semantic` from `pub(crate)`/`pub(super)` to `pub`, so OpenRewrite can
consume its types.

Ruff sources are never edited by hand. Every change this fork makes is encoded as
a rule in `widen_ty_visibility.sh`, and the commit on top of upstream is generated
output. Syncing re-derives that commit against new upstream code rather than
rebasing a large diff, so there is nothing to conflict.

The branch is therefore exactly two commits on top of `upstream/main`:

1. **Script commit** — `Add widen_ty_visibility.sh script`. Authored; holds this
    file and the sync script. Its own message documents day-to-day usage.
1. **Visibility commit** — `Widen ty_python_semantic visibility to pub`.
    Generated; never edit it, and never commit anything else on top.

## Syncing

```sh
scripts/widen_ty_visibility.sh          # sync
scripts/widen_ty_visibility.sh --test   # sync, then run the tests
```

## Adding a change

Add a rule to the "Fix-ups" section of the script. A rule states the condition
that must hold *after* it runs rather than the edit it makes, because upstream
sometimes adopts a widening itself and the resulting no-op is still correct:

```sh
fixup "<what>" "$TARGET/src/<file>.rs" \
    '<sed expression>' \
    '<regex that must match afterwards>' [min-matches]

fixup_absent "<what>" "$TARGET/src/<file>.rs" \
    '<sed expression>' \
    '<regex that must match nothing afterwards>'
```

Blanket widening covers most items, so a rule is only needed for something it
cannot express: a bare `fn` or `mod` with no visibility qualifier to rewrite, a
`#[salsa::interned]` field, or an item that must stay private.

## When a sync fails

The script reports every rule whose condition no longer holds and stops before
committing, restoring the crate. Retarget those rules and re-run. Uncommitted
edits to the script commit's files are allowed for exactly this loop, and are
folded into the new script commit.

A rule that fails this way is the design working: because the script is the only
place this fork's changes exist, a rule that silently stopped matching would drop
a change that nothing would notice until a consumer failed to build.

## Finding what upstream made unusable

Each sync ends with a report of types that appear in a public signature without
being public themselves, which rustc detects as `private_interfaces` /
`private_bounds`. A consumer crate cannot call such an item, so this is the
defect the fork exists to prevent, and upstream adds more of them over time:
the blanket widening rewrites `pub(crate)` and `pub(super)`, but a bare `struct`
or `enum` has no qualifier to rewrite and stays private.

Most hits are internals nobody consumes, so the report never widens on its own.
Add a rule for any a consumer needs.

## Adding a file

Add it to the script commit. The sync reads that commit's file list and carries
those files across. Everything else is destroyed by the reset, so a change that
cannot be expressed as a rule cannot survive here.
