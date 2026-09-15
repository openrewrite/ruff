#!/usr/bin/env bash
set -euo pipefail

# Re-derives this fork's changes on top of the latest upstream/main.
#
# See scripts/CLAUDE.md for the full design; `--help` prints a summary.

REPO_ROOT="$(git rev-parse --show-toplevel)"
TARGET="$REPO_ROOT/crates/ty_python_semantic"

VIS_COMMIT_MSG="Widen ty_python_semantic visibility to pub"
SCRIPT_COMMIT_MSG="Add widen_ty_visibility.sh script"

# Bootstraps a branch that has no script commit yet; step 0 reads the real list.
DEFAULT_SCRIPT_FILES=(
    "scripts/widen_ty_visibility.sh"
    "scripts/CLAUDE.md"
)

usage() {
    cat <<'EOF'
Usage: scripts/widen_ty_visibility.sh [--test]

Re-derives this fork's changes on top of the latest upstream/main:
drops the generated visibility commit, resets to upstream/main, then
re-applies the widening and commits the result.

  --test    Also run the ty_python_semantic tests after cargo check
EOF
}

run_tests=false
for arg in "$@"; do
    case "$arg" in
        --test) run_tests=true ;;
        --help|-h) usage; exit 0 ;;
        *) echo "Unknown option: $arg" >&2; usage >&2; exit 1 ;;
    esac
done

# ── Fix-up helpers ───────────────────────────────────────────────────
# A fix-up asserts the condition that must hold after it runs, so upstream
# adopting a widening itself stays a silent no-op while upstream moving the item
# fails loudly.  See CLAUDE.md: "When a sync fails".
failed_fixups=()

count_matches() {
    local file="$1" regex="$2"
    [[ -f "$file" ]] || { echo 0; return; }
    grep -cE "$regex" "$file" 2>/dev/null || true
}

# fixup <desc> <file> <sed-expr> <expect-regex> [min-matches]
#   Applies <sed-expr>, then requires <expect-regex> on at least
#   [min-matches] lines (default 1).
fixup() {
    local desc="$1" file="$2" expr="$3" expect="$4" min="${5:-1}"
    if [[ ! -f "$file" ]]; then
        failed_fixups+=("$desc: file not found: ${file#"$REPO_ROOT"/}")
        return
    fi
    sed -i '' "$expr" "$file"
    local n
    n="$(count_matches "$file" "$expect")"
    if (( n < min )); then
        failed_fixups+=("$desc: expected >=$min match(es) of '$expect' in ${file#"$REPO_ROOT"/}, found $n")
    fi
}

# fixup_absent <desc> <file> <sed-expr> <forbid-regex>
#   Applies <sed-expr>, then requires <forbid-regex> to match nothing.
fixup_absent() {
    local desc="$1" file="$2" expr="$3" forbid="$4"
    if [[ ! -f "$file" ]]; then
        failed_fixups+=("$desc: file not found: ${file#"$REPO_ROOT"/}")
        return
    fi
    sed -i '' "$expr" "$file"
    local n
    n="$(count_matches "$file" "$forbid")"
    if (( n > 0 )); then
        failed_fixups+=("$desc: '$forbid' still matches $n line(s) in ${file#"$REPO_ROOT"/}")
    fi
}

# Reports types that appear in a public signature without being public
# themselves, which a consumer crate cannot call.  Most are internals, so this
# reports rather than widens.  See CLAUDE.md: "Finding what upstream made
# unusable".
report_private_in_public() {
    local log="$1" tmp
    tmp="$(mktemp)"
    # Module segments are snake_case, so a lowercase initial tells a path prefix
    # to drop apart from an associated type like `PathVisitor::Break` to keep.
    grep -ohE '`[^`]+` is more private than the item `[^`]+`' "$log" 2>/dev/null \
        | sed -E 's/^`([^`]+)` is more private than the item `([^`]+)`$/\1\t\2/' \
        | awk -F'\t' '{ t=$1; gsub(/^([a-z_][A-Za-z0-9_]*::)+/, "", t); gsub(/<[^>]*>/, "", t); print t "\t" $2 }' \
        | sort -u | awk -F'\t' '!seen[$1]++' > "$tmp"

    local n
    n="$(wc -l < "$tmp" | tr -d ' ')"
    if [[ "$n" -gt 0 ]]; then
        echo ""
        echo "Public API unusable outside the crate ($n type(s)):"
        while IFS=$'\t' read -r ty item; do
            printf '  - %s\n      exposed by %s\n' "$ty" "$item"
        done < "$tmp"
        echo "Add a fixup rule for any of these a consumer needs."
    fi
    rm -f "$tmp"
}

# ── Step 0: Fetch, then validate branch shape ────────────────────────
echo "Step 0: Fetching upstream..."
git fetch upstream

# The branch must be exactly the generated commits on top of an upstream
# commit.  Anything else means work that the reset below would destroy, since
# only the script commit's files are carried across.
head_msg="$(git log -1 --format=%s)"
if [[ "$head_msg" == "$VIS_COMMIT_MSG" ]]; then
    script_commit="HEAD~1"
    base="HEAD~2"
elif [[ "$head_msg" == "$SCRIPT_COMMIT_MSG" ]]; then
    script_commit="HEAD"
    base="HEAD~1"
else
    script_commit=""
    base="HEAD"
fi

if [[ -n "$script_commit" ]] \
   && [[ "$(git log -1 --format=%s "$script_commit")" != "$SCRIPT_COMMIT_MSG" ]]; then
    echo "Error: expected '$SCRIPT_COMMIT_MSG' at $script_commit, found:" >&2
    echo "  $(git log -1 --format=%s "$script_commit")" >&2
    exit 1
fi

if ! git merge-base --is-ancestor "$base" upstream/main; then
    echo "Error: $base is not an upstream commit, so this branch carries work" >&2
    echo "beyond the generated commits.  Rebase or drop it before syncing:" >&2
    git --no-pager log --oneline "$(git merge-base "$base" upstream/main)".."$base" >&2
    exit 1
fi

# Read the carried file list from the script commit, so that adding a file to
# that commit is enough to keep it across syncs.
SCRIPT_FILES=()
if [[ -n "$script_commit" ]]; then
    while IFS= read -r f; do
        [[ -n "$f" ]] && SCRIPT_FILES+=("$f")
    done < <(git show --name-only --format= "$script_commit")
fi
if [[ ${#SCRIPT_FILES[@]} -eq 0 ]]; then
    SCRIPT_FILES=("${DEFAULT_SCRIPT_FILES[@]}")
fi
echo "Carrying forward: ${SCRIPT_FILES[*]}"

# Uncommitted changes are tolerated in the carried files, so a failed sync can
# be fixed by editing a rule and re-running; the edits land in the new script
# commit.  Changes anywhere else would be lost, so refuse to run.
dirty="$(git status --porcelain --untracked-files=no)"
if [[ -n "$dirty" ]]; then
    while IFS= read -r line; do
        path="${line:3}"
        for allowed in "${SCRIPT_FILES[@]}"; do
            [[ "$path" == "$allowed" ]] && continue 2
        done
        echo "Error: uncommitted changes outside the carried files:" >&2
        echo "$dirty" >&2
        exit 1
    done <<< "$dirty"
    echo "Note: carrying over uncommitted edits to the carried files."
fi

# ── Step 1: Reset to upstream/main ───────────────────────────────────
# The carried files are copied out first: they are tracked, so a hard reset
# would otherwise take the working-tree copy along with the commit.
STAGING="$(mktemp -d)"
trap 'rm -rf "$STAGING"' EXIT
for f in "${SCRIPT_FILES[@]}"; do
    if [[ -f "$REPO_ROOT/$f" ]]; then
        mkdir -p "$STAGING/$(dirname "$f")"
        cp -p "$REPO_ROOT/$f" "$STAGING/$f"
    fi
done

echo "Step 1: Resetting to upstream/main ($(git log -1 --format=%h upstream/main))..."
git reset -q --hard upstream/main

# ── Step 2: Restore and commit the carried files ─────────────────────
echo "Step 2: Committing carried files..."
for f in "${SCRIPT_FILES[@]}"; do
    if [[ -f "$STAGING/$f" ]]; then
        mkdir -p "$REPO_ROOT/$(dirname "$f")"
        cp -p "$STAGING/$f" "$REPO_ROOT/$f"
    fi
done

git add -- "${SCRIPT_FILES[@]}"
git commit -q -m "$(cat <<'EOF'
Add widen_ty_visibility.sh script

This branch is a fork of astral-sh/ruff that widens visibility in
ty_python_semantic from pub(crate)/pub(super) to pub, so OpenRewrite can
consume its types.

Ruff sources are never edited by hand.  Every change this fork makes is
encoded as a rule in scripts/widen_ty_visibility.sh, and the commit on
top of upstream is generated output.  Syncing therefore re-derives that
commit against new upstream code instead of rebasing a large diff, so
there is nothing to conflict.

Syncing with upstream
    scripts/widen_ty_visibility.sh           sync
    scripts/widen_ty_visibility.sh --test    sync, then run the tests

This resets to upstream/main, re-applies the rules and commits the
result, leaving exactly two commits on top of upstream: this one and the
generated one.

Adding a change
Add a rule to the "Fix-ups" section.  Each rule states the condition that
must hold after it runs, not the edit it makes, because upstream
sometimes adopts a widening itself and the resulting no-op is still
correct:

    fixup        "<what>" "<file>" '<sed>' '<must match after>' [min]
    fixup_absent "<what>" "<file>" '<sed>' '<must match nothing after>'

When upstream moves an item
The sync reports every rule whose condition no longer holds and stops
before committing, restoring the crate.  Retarget those rules and re-run;
uncommitted edits to the carried files are allowed for exactly this loop
and are folded into the new script commit.

Finding what upstream made unusable
Each sync ends with a report of types that appear in a public
signature without being public themselves, which a consumer crate
cannot call.  Most are internals nobody consumes, so it reports rather
than widens; add a rule for any a consumer needs.

Any file added to this commit is carried across future syncs
automatically.  Anything else is destroyed by the reset, so a change that
cannot be expressed as a rule cannot survive here.
EOF
)"

# ── Step 3: Widen visibility ─────────────────────────────────────────
echo "Step 3: Widening visibility in ty_python_semantic..."

# Blanket widen: convert all restricted visibility qualifiers to `pub`.
find "$TARGET" -name '*.rs' -exec sed -i '' \
    -e 's/pub(super)/pub/g' \
    -e 's/pub(crate)/pub/g' \
    -e 's/pub(in [^)]*)/pub/g' \
    {} +

# ── Fix-ups ──────────────────────────────────────────────────────────
# Items that can't simply become `pub`, or that the blanket sed misses.

# `todo_type` is a macro_rules! macro — can't be `pub use` without #[macro_export].
fixup "todo_type stays crate-private" "$TARGET/src/types.rs" \
    's/^pub use todo_type;$/pub(crate) use todo_type;/' \
    '^pub\(crate\) use todo_type;$'

# `SynthesizedProtocolType` is re-exported from a private module.
fixup "synthesized_protocol module" "$TARGET/src/types/instance.rs" \
    's/^mod synthesized_protocol {$/pub mod synthesized_protocol {/' \
    '^pub mod synthesized_protocol \{$'

# The remaining fix-ups widen bare `fn`s and `mod`s, which carry no visibility
# qualifier for the blanket sed to rewrite.

# Consumers call `Type::bindings()` to resolve a call.
fixup "Type::bindings" "$TARGET/src/types.rs" \
    '/^    fn bindings(self, db/s/^    fn /    pub fn /' \
    '^    pub fn bindings\(self, db'

# `Type::apply_specialization()` specializes a parameter's annotated type
# against the specialization a call binding inferred.
fixup "Type::apply_specialization" "$TARGET/src/types.rs" \
    '/^    fn apply_specialization($/s/^    fn /    pub fn /' \
    '^    pub fn apply_specialization\($'

# `CallableBinding::return_type` and `Binding::return_type` both let consumers
# read the return type of the single overload they selected; the public
# `Bindings::return_type` unions across all overloads instead.
fixup "CallableBinding/Binding::return_type" "$TARGET/src/types/call/bind.rs" \
    "/^    fn return_type(&self) -> Type<'db> {\$/s/^    fn /    pub fn /" \
    "^    pub fn return_type\(&self\) -> Type<'db> \{\$" 2

# `TypeIsType::return_type` exposes a `TypeIs`'s narrowed type.  Matching the
# trailing `{` confines this to the inherent method, since the `TypeGuardLike`
# declaration ends in `;`.
fixup "TypeIsType::return_type" "$TARGET/src/types.rs" \
    "/^    fn return_type(self, db: &'db dyn Db) -> Type<'db> {\$/s/^    fn /    pub fn /" \
    "^    pub fn return_type\(self, db: &'db dyn Db\) -> Type<'db> \{\$"

# `KnownClass::canonical_module` maps a known class to its defining module.
fixup "KnownClass::canonical_module" "$TARGET/src/types/class/known.rs" \
    "/^    fn canonical_module(self, python_version: PythonVersion) -> KnownModule {\$/s/^    fn /    pub fn /" \
    '^    pub fn canonical_module\(self, python_version: PythonVersion\) -> KnownModule \{$'

# Bare `mod` declarations in types.rs.
fixup_absent "types.rs submodules are public" "$TARGET/src/types.rs" \
    's/^mod \([a-z_]*;\)/pub mod \1/' \
    '^mod [a-z_]*;$'

# Consumers call `dunder_all::dunder_all_names` for __all__-aware public-API
# filtering.
fixup "dunder_all module" "$TARGET/src/lib.rs" \
    's/^mod dunder_all;$/pub mod dunder_all;/' \
    '^pub mod dunder_all;$'

# `TypeGuardType` and `TypeFormType` are `#[salsa::interned]`, so a field
# without `pub` generates a crate-private accessor.  Consumers read the guarded
# type directly rather than through the `TypeGuardLike` trait, which conflates
# the raw `TypeIs.type_argument` with the top-materialized `TypeIs.return_type`.
fixup "TypeGuardType::return_type" "$TARGET/src/types.rs" \
    '/^pub struct TypeGuardType<.db> {/,/^}/ s/^    return_type:/    pub return_type:/' \
    '^    pub return_type: Type<.db>,$'

fixup "TypeFormType::type_argument" "$TARGET/src/types/type_form.rs" \
    '/^pub struct TypeFormType<.db> {/,/^}/ s/^    type_argument:/    pub type_argument:/' \
    '^    pub type_argument: Type<.db>,$'

# ── Report failed fix-ups ────────────────────────────────────────────
if [[ ${#failed_fixups[@]} -gt 0 ]]; then
    echo "" >&2
    echo "Error: ${#failed_fixups[@]} fix-up(s) no longer hold, most likely because" >&2
    echo "upstream moved or renamed the item:" >&2
    printf '  - %s\n' "${failed_fixups[@]}" >&2
    echo "" >&2
    echo "Retarget them in scripts/widen_ty_visibility.sh and re-run; the crate has" >&2
    echo "been restored, and your edits to the script are carried over." >&2
    git checkout -- "$TARGET"
    exit 1
fi

# ── Format & commit ──────────────────────────────────────────────────
echo "Running cargo fmt -p ty_python_semantic..."
cargo fmt -p ty_python_semantic

git add -- "$TARGET"
git commit -q -m "$VIS_COMMIT_MSG"
echo "Created visibility commit."

# ── Step 4: Verify ───────────────────────────────────────────────────
# --all-targets so the test modules, which the blanket sed also rewrites, are
# type-checked too.
echo "Step 4: Running cargo check -p ty_python_semantic --all-targets..."
check_log="$(mktemp)"
trap 'rm -rf "$STAGING" "$check_log"' EXIT
cargo check -p ty_python_semantic --all-targets 2>&1 | tee "$check_log"
report_private_in_public "$check_log"

if $run_tests; then
    if cargo nextest --version >/dev/null 2>&1; then
        echo "Running cargo nextest run -p ty_python_semantic..."
        cargo nextest run -p ty_python_semantic
    else
        echo "Running cargo test -p ty_python_semantic..."
        cargo test -p ty_python_semantic
    fi
fi

echo ""
echo "Done. History:"
git --no-pager log --oneline -3
