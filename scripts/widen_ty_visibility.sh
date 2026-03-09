#!/usr/bin/env bash
set -euo pipefail

# Syncs this fork with upstream/main and re-applies the custom commits.
#
# This branch (ty-types-2) keeps two custom commits on top of upstream/main:
#   1. "Add widen_ty_visibility.sh script"  — this script + CLAUDE.md
#   2. "Widen ty_python_semantic visibility to pub" — blanket pub widening
#
# The sync workflow:
#   - Drops the visibility commit (if present)
#   - Un-commits the script commit (keeping files in working tree)
#   - Hard-resets to upstream/main (untracked files survive)
#   - Re-commits the script files
#   - Re-applies the visibility widening
#
# Usage: scripts/widen_ty_visibility.sh [--test]
#   --test    Also run tests after cargo check

REPO_ROOT="$(git rev-parse --show-toplevel)"
TARGET="$REPO_ROOT/crates/ty_python_semantic"

VIS_COMMIT_MSG="Widen ty_python_semantic visibility to pub"
SCRIPT_COMMIT_MSG="Add widen_ty_visibility.sh script"

SCRIPT_FILES=(
    "scripts/widen_ty_visibility.sh"
    "scripts/CLAUDE.md"
)

# Parse args
run_tests=false
for arg in "$@"; do
    case "$arg" in
        --test) run_tests=true ;;
        --help|-h)
            sed -n '4,18s/^# \?//p' "$0"
            exit 0
            ;;
        --*) echo "Unknown option: $arg"; exit 1 ;;
    esac
done

# ── Step 0: Validate ─────────────────────────────────────────────────
if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "Error: working tree is not clean."
    exit 1
fi

# ── Step 1: Drop visibility commit ───────────────────────────────────
last_msg="$(git log -1 --format=%s)"
if [[ "$last_msg" == "$VIS_COMMIT_MSG" ]]; then
    echo "Step 1: Dropping visibility commit..."
    git reset --hard HEAD~1
else
    echo "Step 1: No visibility commit to drop (HEAD: $last_msg)"
fi

# ── Step 2: Un-commit script commit ──────────────────────────────────
last_msg="$(git log -1 --format=%s)"
if [[ "$last_msg" == "$SCRIPT_COMMIT_MSG" ]]; then
    echo "Step 2: Un-committing script commit (files stay in working tree)..."
    git reset --mixed HEAD~1
else
    echo "Step 2: No script commit to un-commit (HEAD: $last_msg)"
fi

# ── Step 3: Reset to upstream/main ───────────────────────────────────
echo "Step 3: Fetching upstream and resetting to upstream/main..."
git fetch upstream
git reset --hard upstream/main

# ── Step 4: Re-commit script files ───────────────────────────────────
echo "Step 4: Committing script files..."
cd "$REPO_ROOT"
git add "${SCRIPT_FILES[@]}"
git commit -m "$(cat <<'EOF'
Add widen_ty_visibility.sh script

This branch (ty-types-2) is a fork of astral-sh/ruff that widens
visibility in ty_python_semantic from pub(crate)/pub(super) to pub
for consumption by OpenRewrite.

To sync with upstream:  scripts/widen_ty_visibility.sh
To sync and run tests:  scripts/widen_ty_visibility.sh --test
EOF
)"

# ── Step 5: Widen visibility and commit ──────────────────────────────
echo "Step 5: Widening visibility in ty_python_semantic..."

# Blanket widen: convert all restricted visibility qualifiers to `pub`
find "$TARGET" -name '*.rs' -exec sed -i '' \
    -e 's/pub(super)/pub/g' \
    -e 's/pub(crate)/pub/g' \
    -e 's/pub(in [^)]*)/pub/g' \
    {} +

# ── Fix-ups ──────────────────────────────────────────────────────────
# Items that can't simply become `pub`, or that the blanket sed misses.

# `todo_type` is a macro_rules! macro — can't be `pub use` without #[macro_export]
sed -i '' 's/^pub use todo_type;$/pub(crate) use todo_type;/' \
    "$TARGET/src/types.rs"

# `SynthesizedProtocolType` is re-exported from a private module
sed -i '' 's/^mod synthesized_protocol {$/pub mod synthesized_protocol {/' \
    "$TARGET/src/types/instance.rs"

# Type::bindings() is a bare `fn` (no visibility qualifier) — not caught by
# the pub(crate)->pub sed.  Make it public so external crates can call it.
sed -i '' '/^    fn bindings(self, db/s/^    fn /    pub fn /' \
    "$TARGET/src/types.rs"

# Private `mod` declarations in types.rs — the blanket sed only catches
# `pub(crate) mod`, not bare `mod`.  Make them all public.
sed -i '' 's/^mod \([a-z_]*;\)/pub mod \1/' \
    "$TARGET/src/types.rs"

# ── Format & commit ──────────────────────────────────────────────────
echo "Running cargo fmt -p ty_python_semantic..."
cargo fmt -p ty_python_semantic

git add "$TARGET"
git commit -m "$VIS_COMMIT_MSG"
echo "Created visibility commit."

# ── Step 6: Verify ───────────────────────────────────────────────────
echo "Step 6: Running cargo check -p ty_python_semantic..."
cargo check -p ty_python_semantic

if $run_tests; then
    echo "Running cargo test -p ty_python_semantic..."
    cargo test -p ty_python_semantic
fi

echo ""
echo "Done. History:"
git log --oneline -3
