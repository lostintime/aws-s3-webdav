#!/bin/sh

SCRIPTPATH="$( cd "$(dirname "$0")" ; pwd -P )"

docker build -f "$SCRIPTPATH/Dockerfile-alpine" -t "lostintime/aws-s3-webdav:latest-alpine" --squash $SCRIPTPATH
