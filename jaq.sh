#!/bin/bash
#set -ex

JAQ_PROGRAM=$1

if [[ -z $JAQ_PROGRAM ]];
then
    echo "Missing required argument"
    echo "Usage: ./build.sh JAQ_PROGRAM"
    exit 1
fi

export JAQ_PROGRAM;
cargo build --release -p jaq-http --target wasm32-wasip2
if [ ! -x "$(command -v component-init)" ];
then
    cargo install --git https://github.com/dicej/component-init component-init-cli
fi
component-init target/wasm32-wasip2/release/jaq-http.wasm -o jaq-http.wasm
