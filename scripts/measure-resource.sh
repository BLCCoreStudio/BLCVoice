#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: scripts/measure-resource.sh -- COMMAND [ARG ...]

Runs an already-built benchmark/program under the platform-native /usr/bin/time
resource probe. The command's stdout is preserved. BLCVoice resource metadata is
appended to stdout and the native time report is preserved on stderr.
EOF
}

if [[ ${1:-} != "--" || $# -lt 2 ]]; then
  usage
  exit 2
fi
shift

if [[ ! -x /usr/bin/time ]]; then
  echo "resource probe unavailable: /usr/bin/time is not executable" >&2
  exit 2
fi

tmp_dir=$(mktemp -d)
command_stderr="$tmp_dir/command.stderr"
time_stderr="$tmp_dir/time.stderr"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

run_timed() {
  local mode=$1
  shift
  set +e
  /usr/bin/time "$mode" sh -c 'command_stderr=$1; shift; exec "$@" 2>"$command_stderr"' \
    blcvoice-resource-probe "$command_stderr" "$@" 2>"$time_stderr"
  local status=$?
  set -e
  return "$status"
}

platform=$(uname -s)
status=0
case "$platform" in
  Linux)
    run_timed -v "$@" || status=$?
    peak_value=$(awk -F': ' '/Maximum resident set size \(kbytes\)/ { print $2; exit }' "$time_stderr")
    metric_name="linux_peak_resident_set_kib"
    metric_semantics="gnu-time maximum resident set size (kbytes); Linux wait4/getrusage semantics"
    ;;
  Darwin)
    run_timed -l "$@" || status=$?
    peak_value=$(awk '/maximum resident set size/ { print $1; exit }' "$time_stderr")
    metric_name="macos_peak_resident_set_bytes"
    metric_semantics="macOS time -l maximum resident set size; native bytes"
    ;;
  *)
    echo "resource probe unavailable on uname=$platform; use the Windows PowerShell wrapper on Windows" >&2
    exit 2
    ;;
esac

cat "$command_stderr" >&2
cat "$time_stderr" >&2

if [[ -z ${peak_value:-} || ! $peak_value =~ ^[0-9]+$ ]]; then
  echo "resource probe failed: native peak resident-set metric was not parseable" >&2
  exit 1
fi

printf '%s\n' \
  "resource_format=blcvoice-resource-evidence-v1" \
  "resource_platform=$platform" \
  "resource_metric=$metric_name" \
  "resource_value=$peak_value" \
  "resource_semantics=$metric_semantics" \
  "resource_command_exit_code=$status"

exit "$status"
