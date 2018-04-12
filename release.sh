#!/bin/sh

SCRIPTPATH="$( cd "$(dirname "$0")" ; pwd -P )"

./cargo.sh clean

./cargo.sh build --release --target x86_64-unknown-linux-gnu
