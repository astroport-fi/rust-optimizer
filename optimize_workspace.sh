#!/bin/ash
# shellcheck shell=dash
# See https://www.shellcheck.net/wiki/SC2187
set -o errexit -o nounset -o pipefail
command -v shellcheck >/dev/null && shellcheck "$0"

export PATH="$PATH:/root/.cargo/bin"

# Suffix for non-Intel built artifacts
MACHINE=$(uname -m)
SUFFIX=${MACHINE#x86_64}
SUFFIX=${SUFFIX:+-$SUFFIX}

rustup toolchain list
cargo --version

# Delete already built artifacts
rm -f target/wasm32-unknown-unknown/release/*.wasm

# Build artifacts

feat_location="/code/features.json"
get_features() {
  set -e
  if [ -f "$feat_location" ]; then
    jq -r ".\"$1\" | select(. != null)" <"$feat_location"
  fi
  return 0
}

echo "Building artifacts in workspace..."

ws_members="$(cargo metadata --no-deps --locked --format-version 1 |
  jq -r ".packages[] | select(.manifest_path | startswith(\"$PWD/contracts\")) | .name")"
echo -e "Contracts to be built:\n$ws_members"
for member in $ws_members; do
  features="$(get_features "$member")"
  if [ -n "$features" ]; then
    echo "Building $member with enabled features: $features ..."
    RUSTFLAGS='-C link-arg=-s' cargo build -p "$member" --release --features "$features" --lib --target wasm32-unknown-unknown --locked
  else
    echo "Building $member ..."
    RUSTFLAGS='-C link-arg=-s' cargo build -p "$member" --release --lib --target wasm32-unknown-unknown --locked
  fi
done

mkdir -p artifacts
echo "Optimizing artifacts in workspace..."
TMPARTIFACTS=$(mktemp -p "$(pwd)" -d artifacts.XXXXXX)
# Optimize artifacts
(
  cd "$TMPARTIFACTS"
  INTERMEDIATE_SHAS="../artifacts/checksums_intermediate.txt"
  OPTIMIZED_SHAS="../artifacts/checksums.txt"

  for WASM in ../target/wasm32-unknown-unknown/release/*.wasm; do
    BASENAME=$(basename "$WASM" .wasm)
    NAME=${BASENAME}${SUFFIX}
    OPTIMIZED_WASM=${NAME}.wasm

    INTERMEDIATE_SHA=$(sha256sum -- "$WASM" | sed 's,../target,target,g')

    SKIP_OPTIMIZATION=false
    if test -f "../artifacts/${OPTIMIZED_WASM}"; then
      INTERMEDIATE_CACHE_HIT=$(
        grep -Fxsq "$INTERMEDIATE_SHA" "$INTERMEDIATE_SHAS"
        echo $?
      )
      OPTIMIZED_SHA=$(sha256sum -- "../artifacts/$OPTIMIZED_WASM" | sed 's,../artifacts/,,g')
      OPTIMIZED_CACHE_HIT=$(
        grep -Fxsq "$OPTIMIZED_SHA" "$OPTIMIZED_SHAS"
        echo $?
      )
      if [ "$INTERMEDIATE_CACHE_HIT" -eq 0 ] && [ "$OPTIMIZED_CACHE_HIT" -eq 0 ]; then
        SKIP_OPTIMIZATION=true
      fi
    fi

    if [ "$SKIP_OPTIMIZATION" = true ]; then
      echo "$BASENAME unchanged. Skipping optimization."
    else
      if test -f "$INTERMEDIATE_SHAS"; then
        echo "Updating intermediate hash for ${BASENAME}..."
        sed -ni "/$BASENAME/!p" "$INTERMEDIATE_SHAS"
      else
        echo "Creating intermediate hash for ${BASENAME}..."
      fi
      echo "$INTERMEDIATE_SHA" >>"$INTERMEDIATE_SHAS"

      echo "Optimizing ${BASENAME}..."
      wasm-opt -Os "$WASM" -o "$OPTIMIZED_WASM"
      echo "Moving ${OPTIMIZED_WASM}..."
      mv "$OPTIMIZED_WASM" ../artifacts
    fi
  done
)
rm -rf "$TMPARTIFACTS"
echo "Post-processing artifacts in workspace..."
(
  cd artifacts
  sha256sum -- *.wasm | tee checksums.txt
)

echo "done"
