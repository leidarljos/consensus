#!/bin/bash
# Wait until the workflows that run on every push to main have succeeded
# on this commit. release.yml and publish.yml both call this.
set -eu
# ISO-8601 timestamps sort in byte order in the C locale.
export LC_ALL=C
sha=$(git rev-parse HEAD)
echo "sha=$sha" >> "$GITHUB_OUTPUT"
echo "tag $TAG is $sha"
# binstall asks for v{version}. The tag has to be that version
# or the archive name and the metadata name a different file.
crate=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml)
test -n "$crate"
test "v$crate" = "$TAG"

# A workflow that pushes to main with no paths filter runs on
# every commit, so the tag can wait for it. A paths filter skips
# commits that do not touch those paths. release.yml and
# publish.yml run on the tag, not as CI for the commit.
ci_files=()
for f in .github/workflows/*.yml; do
  block=$(awk '
    /^on:/ { on=1 }
    on && /^[^[:space:]#]/ && !/^on:/ { exit }
    on { print }
  ' "$f")
  if [[ "$block" =~ (^|$'\n')[[:space:]]*paths: ]]; then
    continue
  fi
  [[ "$block" == *push:* ]] || continue
  [[ "$block" == *branches:* ]] || continue
  [[ "$block" =~ (^|[^[:alnum:]_])main([^[:alnum:]_]|$) ]] || continue
  ci_files+=("$(basename "$f")")
done
if [ "${#ci_files[@]}" -eq 0 ]; then
  echo "no workflow runs on every push to main" >&2
  exit 1
fi
echo "CI: ${ci_files[*]}"

for wf in "${ci_files[@]}"; do
  best_id=
  best_at=
  # The newest push, pull request, or manual run on this commit.
  # A scheduled re-run is a later advisory check, not that CI.
  # --limit 1 is the newest run of that event, which is what
  # `gh run list` returns first.
  for event in push pull_request workflow_dispatch; do
    row=$(gh run list --workflow "$wf" --commit "$sha" --event "$event" --limit 1 \
      --json databaseId,createdAt --jq '.[0] // empty')
    if [ -z "$row" ]; then
      continue
    fi
    at=$(printf '%s\n' "$row" | jq -r '.createdAt')
    rid=$(printf '%s\n' "$row" | jq -r '.databaseId')
    if [ -z "$best_at" ] || [[ "$at" > "$best_at" ]]; then
      best_at=$at
      best_id=$rid
    fi
  done
  if [ -z "$best_id" ]; then
    echo "$wf has no run on $sha" >&2
    exit 1
  fi
  echo "watch $wf $best_id"
  # gh waits out a run that is still going and returns at once
  # when it has finished. --exit-status stops the release when
  # that run did not succeed. The interval is gh's own.
  gh run watch "$best_id" --exit-status --compact
done
