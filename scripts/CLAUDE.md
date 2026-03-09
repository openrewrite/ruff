# Fork Maintenance: ty-types-2

This branch keeps two custom commits on top of `upstream/main`:

1. **Script commit** — `Add widen_ty_visibility.sh script`
   Contains this file and the sync script.

2. **Visibility commit** — `Widen ty_python_semantic visibility to pub`
   Blanket-widens all `pub(crate)` / `pub(super)` to `pub` in `ty_python_semantic`,
   plus targeted fix-ups for items the blanket sed misses.

## Syncing with upstream

```sh
scripts/widen_ty_visibility.sh          # sync only
scripts/widen_ty_visibility.sh --test   # sync + run tests
```

The script:
1. Drops the visibility commit
2. Un-commits the script commit (files stay in working tree)
3. Hard-resets to `upstream/main` (no merge commits)
4. Re-commits the script files
5. Re-applies visibility widening via sed + fix-ups + cargo fmt
6. Runs `cargo check` (and `cargo test` with `--test`)

## Adding new fix-ups

If upstream introduces new items that break compilation after the blanket
`pub(crate)` → `pub` widening, add a targeted sed command in the "Fix-ups"
section of `widen_ty_visibility.sh`.
