#!/bin/sh

SCRIPTPATH="$( cd "$(dirname "$0")" ; pwd -P )"

docker build -f "$SCRIPTPATH/Dockerfile" -t "lostintime/aws-s3-webdav:latest" --squash $SCRIPTPATH
