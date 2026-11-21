#!/bin/sh
set -eu

model_dir=$(CDPATH='' cd "$(dirname "$0")" && pwd)
version=$(z3 --version)
case "$version" in
  "Z3 version 4.13.3 - "*) ;;
  *)
    printf 'Expected Z3 4.13.3, found %s\n' "$version" >&2
    exit 1
    ;;
esac

if ! result=$(z3 -smt2 "$model_dir/timeline.smt2"); then
  printf '%s\n' "$result" >&2
  exit 1
fi
printf '%s\n' "$result" | diff -u "$model_dir/timeline.expected" -
printf '%s\n' "$result"
