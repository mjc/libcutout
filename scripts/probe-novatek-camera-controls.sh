#!/usr/bin/env bash
set -euo pipefail

readonly origin='http://192.168.1.254'
readonly response_limit_bytes=$((1024 * 1024))
readonly firmware_command=3012
readonly configuration_command=3014
readonly storage_command=3024
readonly recording_command=2001
readonly still_command=1001
readonly media_command=3015

usage() {
  cat <<'USAGE'
usage: probe-novatek-camera-controls.sh [--dry-run | --self-test | --confirm-r3v1 LOG_FILE]

Run the explicit R3V1 recording/still validation sequence while connected to
the camera Wi-Fi. Metadata and control requests have no timeout; each response
is captured until its XML end tag or the fixed response bound is reached.

The control sequence is refused unless --confirm-r3v1 is supplied. It first
proves firmware command 3012 reports R3V1 and that command 3014 advertises
recording command 2001 with status 0. It then records 2001 start/stop responses
and 3014 readback. Still command 1001 is sent only when 3014 advertises it.
USAGE
}

print_requests() {
  printf '%s\n' \
    "firmware GET $origin/?custom=1&cmd=$firmware_command" \
    "configuration GET $origin/?custom=1&cmd=$configuration_command" \
    "storage GET $origin/?custom=1&cmd=$storage_command" \
    "record-start GET $origin/?custom=1&cmd=$recording_command&str=1" \
    "configuration-after-start GET $origin/?custom=1&cmd=$configuration_command" \
    "record-stop GET $origin/?custom=1&cmd=$recording_command&str=0" \
    "configuration-after-stop GET $origin/?custom=1&cmd=$configuration_command" \
    "still GET $origin/?custom=1&cmd=$still_command" \
    "media-list GET $origin/?custom=1&cmd=$media_command"
}

if (($# == 0)); then
  usage >&2
  exit 2
fi

case "$1" in
  --dry-run)
    (($# == 1)) || { usage >&2; exit 2; }
    print_requests
    exit 0
    ;;
  --self-test)
    (($# == 1)) || { usage >&2; exit 2; }
    expected=$'firmware GET http://192.168.1.254/?custom=1&cmd=3012\nconfiguration GET http://192.168.1.254/?custom=1&cmd=3014\nstorage GET http://192.168.1.254/?custom=1&cmd=3024\nrecord-start GET http://192.168.1.254/?custom=1&cmd=2001&str=1\nconfiguration-after-start GET http://192.168.1.254/?custom=1&cmd=3014\nrecord-stop GET http://192.168.1.254/?custom=1&cmd=2001&str=0\nconfiguration-after-stop GET http://192.168.1.254/?custom=1&cmd=3014\nstill GET http://192.168.1.254/?custom=1&cmd=1001\nmedia-list GET http://192.168.1.254/?custom=1&cmd=3015'
    [[ "$(print_requests)" == "$expected" ]]
    echo "Novatek control probe self-test passed"
    exit 0
    ;;
  --confirm-r3v1)
    (($# == 2)) || { usage >&2; exit 2; }
    log_file=$2
    ;;
  -h | --help)
    (($# == 1)) || { usage >&2; exit 2; }
    usage
    exit 0
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac

command -v curl >/dev/null || { echo "curl is required" >&2; exit 127; }
command -v perl >/dev/null || { echo "perl is required" >&2; exit 127; }
if [[ -e "$log_file" ]]; then
  echo "refusing to overwrite existing log: $log_file" >&2
  exit 2
fi

umask 077
scratch_dir="$(mktemp -d "${TMPDIR:-/tmp}/novatek-r3-controls.XXXXXX")"
cleanup() {
  rm -rf -- "$scratch_dir"
}
trap cleanup EXIT

failures=0
request_number=0
last_response_file=''
last_request_status=0

capture_response() {
  local marker="$1" response_file="$2" url="$3"
  local parser_status

  set +e
  NOVATEK_CAPTURE_MARKER="$marker" \
    NOVATEK_CAPTURE_LIMIT_BYTES="$response_limit_bytes" \
    curl \
      --fail-with-body \
      --include \
      --noproxy '*' \
      --proto '=http' \
      --request GET \
      --show-error \
      --silent \
      "$url" 2>&1 |
    NOVATEK_CAPTURE_MARKER="$marker" \
      NOVATEK_CAPTURE_LIMIT_BYTES="$response_limit_bytes" \
      perl -e '
        my $marker = $ENV{"NOVATEK_CAPTURE_MARKER"};
        my $limit = $ENV{"NOVATEK_CAPTURE_LIMIT_BYTES"};
        my $tail = "";
        my $bytes = 0;
        while (sysread(STDIN, my $chunk, 8192)) {
          $bytes += length($chunk);
          exit 2 if $bytes > $limit;
          print $chunk;
          $tail .= $chunk;
          exit 0 if index($tail, $marker) >= 0;
          $tail = substr($tail, -length($marker)) if length($tail) > length($marker);
        }
        exit 1;
      ' >"$response_file"
  local -a pipeline_status=("${PIPESTATUS[@]}")
  set -e

  parser_status="${pipeline_status[1]}"
  if ((parser_status == 0)); then
    return 0
  fi
  return "$parser_status"
}

request() {
  local label="$1" command_id="$2" query="$3" marker="$4"
  local response_file="$scratch_dir/response-$request_number"
  local url="$origin/?custom=1&cmd=$query"
  local status
  request_number=$((request_number + 1))

  {
    printf '\n=== %s ===\n' "$label"
    printf 'request=%s\n\n' "$url"
  } >>"$log_file"

  if capture_response "$marker" "$response_file" "$url"; then
    status=0
  else
    status=$?
    failures=$((failures + 1))
  fi
  if ((status == 0)) && ! perl -0777 -e '
    my $file = shift;
    open my $input, "<:raw", $file or exit 2;
    my $line = <$input> // "";
    exit($line =~ /^HTTP\/\S+\s+2\d\d(?:\s|$)/ ? 0 : 1);
  ' "$response_file"; then
    status=22
    failures=$((failures + 1))
  fi
  cat "$response_file" >>"$log_file"
  printf '\n\nrequest_exit=%s\nexpected_command=%s\n' "$status" "$command_id" >>"$log_file"
  last_response_file="$response_file"
  last_request_status="$status"
}

has_command_status() {
  local response_file="$1" command_id="$2" status="$3"
  perl -0777 -e '
    my ($file, $command, $status) = @ARGV;
    open my $input, "<:raw", $file or exit 2;
    local $/;
    my $body = <$input>;
    exit($body =~ /<Cmd>\Q$command\E<\/Cmd>\s*<Status>\Q$status\E<\/Status>/ ? 0 : 1);
  ' "$response_file" "$command_id" "$status"
}

has_r3v1_firmware() {
  local response_file="$1"
  perl -0777 -e '
    my $file = shift;
    open my $input, "<:raw", $file or exit 2;
    local $/;
    my $body = <$input>;
    exit($body =~ /<Cmd>3012<\/Cmd>\s*<Status>0<\/Status>\s*<String>R3V1[^<]*<\/String>/ ? 0 : 1);
  ' "$response_file"
}

{
  printf 'Novatek R3V1 control validation\n'
  printf 'captured_at_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'origin=%s\n' "$origin"
  printf 'response_limit_bytes=%s\n' "$response_limit_bytes"
  printf 'network_timeouts=none\n'
  printf 'mutating_commands=2001 start/stop; 1001 still when advertised\n'
} >"$log_file"

request firmware "$firmware_command" "$firmware_command" '</Function>'
firmware_response="$last_response_file"
if ((last_request_status != 0)) || ! has_r3v1_firmware "$firmware_response"; then
  printf 'gate=r3v1-failed\n' >>"$log_file"
  echo "R3V1 firmware gate failed; no mutating command was sent" >&2
  exit 1
fi
printf 'gate=r3v1-passed\n' >>"$log_file"

request configuration "$configuration_command" "$configuration_command" '</Function>'
configuration_response="$last_response_file"
if ((last_request_status != 0)); then
  printf 'configuration_result=incomplete\n' >>"$log_file"
  echo "configuration response was incomplete; no mutating command was sent" >&2
  exit 1
fi

request storage "$storage_command" "$storage_command" '</Function>'
storage_response="$last_response_file"
if ((last_request_status != 0)) || ! has_command_status "$storage_response" "$storage_command" 0; then
  printf 'storage_result=unconfirmed\n' >>"$log_file"
  echo "storage response was not confirmed; no mutating command was sent" >&2
  exit 1
fi
printf 'storage_result=acknowledged\n' >>"$log_file"

if has_command_status "$configuration_response" "$recording_command" 0; then
  request record-start "$recording_command" "$recording_command&str=1" '</Function>'
  record_start_response="$last_response_file"
  record_start_status="$last_request_status"
  if ((record_start_status == 0)); then
    has_command_status "$record_start_response" "$recording_command" 0 || record_start_status=$?
  fi
  printf 'record_start_result=%s\n' "$([[ $record_start_status == 0 ]] && echo acknowledged || echo non-acknowledged)" >>"$log_file"

  request configuration-after-start "$configuration_command" "$configuration_command" '</Function>'
  configuration_after_start="$last_response_file"
  if ((last_request_status == 0)); then
    printf 'record_readback_after_start=configuration-captured; authoritative-state=unproven\n' >>"$log_file"
  else
    printf 'record_readback_after_start=incomplete\n' >>"$log_file"
    failures=$((failures + 1))
  fi

  request record-stop "$recording_command" "$recording_command&str=0" '</Function>'
  record_stop_response="$last_response_file"
  record_stop_status="$last_request_status"
  if ((record_stop_status == 0)); then
    has_command_status "$record_stop_response" "$recording_command" 0 || record_stop_status=$?
  fi
  printf 'record_stop_result=%s\n' "$([[ $record_stop_status == 0 ]] && echo acknowledged || echo non-acknowledged)" >>"$log_file"

  request configuration-after-stop "$configuration_command" "$configuration_command" '</Function>'
  configuration_after_stop="$last_response_file"
  if ((last_request_status == 0)); then
    printf 'record_readback_after_stop=configuration-captured; authoritative-state=unproven\n' >>"$log_file"
  else
    printf 'record_readback_after_stop=incomplete\n' >>"$log_file"
    failures=$((failures + 1))
  fi
else
  printf 'recording_result=not-advertised\n' >>"$log_file"
  echo "R3V1 configuration does not advertise recording command 2001; no record command was sent" >&2
  failures=$((failures + 1))
fi

if has_command_status "$configuration_response" "$still_command" 0; then
  request still "$still_command" "$still_command" '</Function>'
  still_response="$last_response_file"
  still_status="$last_request_status"
  if ((still_status == 0)); then
    has_command_status "$still_response" "$still_command" 0 || still_status=$?
  fi
  printf 'still_result=%s\n' "$([[ $still_status == 0 ]] && echo acknowledged || echo non-acknowledged)" >>"$log_file"
  request media-list "$media_command" "$media_command" '</LIST>'
  media_response="$last_response_file"
  ((last_request_status == 0)) || failures=$((failures + 1))
else
  printf 'still_result=not-advertised\n' >>"$log_file"
fi

printf 'failures=%s\n' "$failures" >>"$log_file"
echo "Wrote $log_file"
if ((failures)); then
  echo "$failures validation step(s) failed; the log contains the captured responses" >&2
  exit 1
fi
