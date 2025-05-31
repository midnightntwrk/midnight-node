#!/bin/bash

# Usage: ./find_run_id.sh <owner/repo> <workflow_name> <artifact_name>
REPO="$1"
WORKFLOW_NAME="$2"
ARTIFACT_NAME="$3"

# Check if all arguments are provided
if [[ -z "$REPO" || -z "$WORKFLOW_NAME" || -z "$ARTIFACT_NAME" ]]; then
  echo "Usage: $0 <owner/repo> <workflow_name> <artifact_name>"
  exit 1
fi

# Fetch the workflow ID for the specified workflow
workflow_id=$(gh api repos/$REPO/actions/workflows | jq -r --arg name "$WORKFLOW_NAME" '.workflows[] | select(.name == $name) | .id')

if [[ -z "$workflow_id" ]]; then
  echo "Workflow '$WORKFLOW_NAME' not found in repository '$REPO'."
  exit 1
fi

# Fetch all completed workflow runs for the specified workflow, ordered by created_at (newest first)
runs=$(gh api repos/$REPO/actions/workflows/$workflow_id/runs --paginate | jq -r '.workflow_runs[] | select(.status == "completed") | .id')

# Iterate over each run to check for the specified artifact, starting from the newest
for run_id in $runs; do
  artifact_check=$(gh api repos/$REPO/actions/runs/$run_id/artifacts | jq -r --arg name "$ARTIFACT_NAME" '.artifacts[] | select(.name == $name)')

 if [[ -n "$artifact_check" ]]; then
    echo "$run_id"
    exit 0
  fi
done

echo "No completed runs with artifact named '$ARTIFACT_NAME' found."
