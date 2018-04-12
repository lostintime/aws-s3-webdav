#!/bin/sh

SCRIPTPATH="$( cd "$(dirname "$0")" ; pwd -P )"

CARGO_ARGS="${@:1}"

echo "/bin/sh -c '/usr/local/cargo/bin/cargo $CARGO_ARGS'"

docker run --rm \
  -v "$SCRIPTPATH:/app" \
  -w "/app" \
  rust:1.25.0 \
  /usr/local/cargo/bin/cargo $CARGO_ARGS
