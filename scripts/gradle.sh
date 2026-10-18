#!/usr/bin/env bash
# Run the wrapper from android/ so that the tools' working-directory droppings (.kotlin/ daemon run
# files, .android/) land in the ignored android/ and never at the repository root.
# Usage: scripts/gradle.sh <gradle args>   (just gradle, just apk …; run in the image)
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../android"
exec ./gradlew "$@"
