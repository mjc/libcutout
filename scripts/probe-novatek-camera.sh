#!/usr/bin/env bash
set -euo pipefail

origin="${NOVATEK_CAMERA_ORIGIN:-http://192.168.1.254}"
origin="${origin%/}"
readonly origin
readonly -a probes=(liveview-format:2019 firmware-version:3012 configuration:3014 media-list:3015 storage-present:3024)
readonly rtsp_capture_seconds=10
readonly rtsp_stream_timeout_seconds=10

usage() {
  echo "usage: $0 [--dry-run | --self-test | LOG_FILE]"
  echo "override the default origin with NOVATEK_CAMERA_ORIGIN=http://HOST[:PORT]"
  echo "run while connected to the camera Wi-Fi; metadata stops at complete responses and video capture is finite"
  echo "RTSP video is saved beside LOG_FILE as LOG_FILE.rtsp.ts, with a 10-second capture and 10-second stream I/O timeout"
}

print_requests() {
  local probe command_id
  for probe in "${probes[@]}"; do
    command_id="${probe##*:}"
    printf '%s cmd=%s %s/?custom=1&cmd=%s\n' \
      "${probe%%:*}" "$command_id" "$origin" "$command_id"
  done
}

host="${origin#http://}"
if [[ "$origin" != http://* || -z "$host" || "$host" == *['/?#']* ]]; then
  echo "origin must be an HTTP origin without a path, query, or fragment" >&2
  exit 2
fi
if (($# > 1)); then
  usage >&2
  exit 2
fi

case "${1:-}" in
  --dry-run)
    print_requests
    exit 0
    ;;
  --self-test)
    readonly expected=$'liveview-format cmd=2019 http://192.168.1.254/?custom=1&cmd=2019\nfirmware-version cmd=3012 http://192.168.1.254/?custom=1&cmd=3012\nconfiguration cmd=3014 http://192.168.1.254/?custom=1&cmd=3014\nmedia-list cmd=3015 http://192.168.1.254/?custom=1&cmd=3015\nstorage-present cmd=3024 http://192.168.1.254/?custom=1&cmd=3024'
    [[ "$origin" == "http://192.168.1.254" && "$(print_requests)" == "$expected" ]]
    echo "Novatek probe self-test passed"
    exit 0
    ;;
  -h | --help)
    usage
    exit 0
    ;;
  --*)
    usage >&2
    exit 2
    ;;
esac

command -v curl >/dev/null || { echo "curl is required" >&2; exit 127; }
command -v perl >/dev/null || { echo "perl is required" >&2; exit 127; }
log_file="${1:-novatek-r3-pro-$(date -u +%Y%m%dT%H%M%SZ).log}"
if [[ -e "$log_file" ]]; then
  echo "refusing to overwrite existing log: $log_file" >&2
  exit 2
fi

umask 077
{
  printf 'Novatek R3 Pro read-only probe\n'
  printf 'captured_at_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'origin=%s\n' "$origin"
  printf 'http_response_limit=none\n'
  printf 'network_timeouts=none\n'
  printf 'completion=xml-end-tags-and-rtsp-response-body\n'
  printf 'rtsp_methods=OPTIONS,DESCRIBE\n'
  printf 'rtsp_video_capture_seconds=%s\n' "$rtsp_capture_seconds"
  printf 'rtsp_video_idle_timeout_seconds=%s\n' "$rtsp_stream_timeout_seconds"
} >"$log_file"

scratch_dir="$(mktemp -d "${TMPDIR:-/tmp}/novatek-r3-probe.XXXXXX")"
cleanup() {
  rm -rf -- "$scratch_dir"
}
trap cleanup EXIT

failures=0

capture_http_response() {
  local marker="$1" response_file="$2"
  shift 2

  set +e
  NOVATEK_CAPTURE_MARKER="$marker" curl "$@" 2>&1 |
    NOVATEK_CAPTURE_MARKER="$marker" perl -e '
      my $marker = $ENV{"NOVATEK_CAPTURE_MARKER"};
      my $tail = "";
      while (sysread(STDIN, my $chunk, 8192)) {
        print $chunk;
        $tail .= $chunk;
        exit 0 if index($tail, $marker) >= 0;
        $tail = substr($tail, -length($marker)) if length($tail) > length($marker);
      }
      exit 1;
    ' >"$response_file"
  local -a pipeline_status=("${PIPESTATUS[@]}")
  set -e

  local curl_status="${pipeline_status[0]}"
  local parser_status="${pipeline_status[1]}"
  capture_curl_raw_status="$curl_status"
  if ((parser_status == 0)); then
    capture_curl_status=0
    return 0
  fi
  capture_curl_status="$curl_status"
  if ((capture_curl_status == 0)); then
    capture_curl_status="$parser_status"
  fi
  return 1
}

for probe in "${probes[@]}"; do
  command_id="${probe##*:}"
  request="$origin/?custom=1&cmd=$command_id"
  response_file="$scratch_dir/http-$command_id"
  {
    printf '\n=== %s cmd=%s ===\n' "${probe%%:*}" "$command_id"
    printf 'request=%s\n\n' "$request"
  } >>"$log_file"

  curl_options=(
    --fail-with-body \
    --include \
    --noproxy '*' \
    --proto '=http' \
    --request GET \
    --show-error \
    --silent
  )

  marker='</Function>'
  if [[ "$command_id" == 2019 || "$command_id" == 3015 ]]; then
    marker='</LIST>'
  fi

  if capture_http_response "$marker" "$response_file" "${curl_options[@]}" "$request"
  then
    status=0
  else
    status=$?
    failures=$((failures + 1))
  fi
  cat "$response_file" >>"$log_file"
  printf '\n\ncurl_exit=%s\ncurl_raw_exit=%s\n' \
    "$status" "$capture_curl_raw_status" >>"$log_file"
done

rtsp_uri="$(sed -En 's|.*<MovieLiveViewLink>(rtsp://[^<]*)</MovieLiveViewLink>.*|\1|p' "$scratch_dir/http-2019" | head -n 1)"
rtsp_host=''
rtsp_port=''
rtsp_path=''
rtsp_port_number=0

if [[ -z "$rtsp_uri" ]]; then
  printf '\n=== rtsp-discovery ===\nresult=missing-movie-live-view-link\n' >>"$log_file"
  failures=$((failures + 1))
else
  camera_host="${host%%:*}"
  if [[ "$rtsp_uri" =~ ^rtsp://([^/:]+)(:([0-9]+))?(/[^[:space:]]*)$ ]]; then
    rtsp_host="${BASH_REMATCH[1]}"
    rtsp_port="${BASH_REMATCH[3]:-554}"
    rtsp_path="${BASH_REMATCH[4]}"
    rtsp_port_number=$((10#$rtsp_port))
  else
    printf '\n=== rtsp-discovery ===\nresult=invalid-uri uri=%s\n' "$rtsp_uri" >>"$log_file"
    failures=$((failures + 1))
    rtsp_uri=''
  fi

  if [[ -n "$rtsp_uri" && "$rtsp_host" != "$camera_host" ]]; then
    printf '\n=== rtsp-discovery ===\nresult=host-mismatch expected=%s actual=%s\n' \
      "$camera_host" "$rtsp_host" >>"$log_file"
    failures=$((failures + 1))
    rtsp_uri=''
  elif [[ -n "$rtsp_uri" ]] && ((rtsp_port_number < 1 || rtsp_port_number > 65535)); then
    printf '\n=== rtsp-discovery ===\nresult=invalid-port port=%s\n' "$rtsp_port" >>"$log_file"
    failures=$((failures + 1))
    rtsp_uri=''
  fi
fi

if [[ -n "$rtsp_uri" ]]; then
  printf '\n=== rtsp-discovery ===\nuri=%s\nhost=%s\nport=%s\npath=%s\n' \
    "$rtsp_uri" "$rtsp_host" "$rtsp_port" "$rtsp_path" >>"$log_file"

  if ! command -v nc >/dev/null; then
    printf 'result=nc-required\n' >>"$log_file"
    failures=$((failures + 1))
  else
    probe_rtsp() {
      local label="$1" method="$2" request_file="$scratch_dir/rtsp-$1-request"
      local response_file="$scratch_dir/rtsp-$1-response" status
      {
        printf '%s %s RTSP/1.0\r\n' "$method" "$rtsp_uri"
        printf 'CSeq: 1\r\n'
        printf 'User-Agent: Cutout-Novatek-Probe/1\r\n'
        if [[ "$method" == DESCRIBE ]]; then
          printf 'Accept: application/sdp\r\n'
        fi
        printf '\r\n'
      } >"$request_file"

      set +e
      cat "$request_file" |
        nc "$rtsp_host" "$rtsp_port" 2>&1 |
        NOVATEK_RTSP_NEED_BODY="$([[ "$method" == DESCRIBE ]] && echo 1 || echo 0)" perl -e '
          my $need_body = $ENV{"NOVATEK_RTSP_NEED_BODY"};
          my $data = "";
          while (sysread(STDIN, my $chunk, 8192)) {
            print $chunk;
            $data .= $chunk;
            my $header_end = index($data, "\r\n\r\n");
            next if $header_end < 0;
            exit 0 if !$need_body;
            my $headers = substr($data, 0, $header_end);
            my ($length) = $headers =~ /^Content-Length:\s*(\d+)/mi;
            exit 0 if !defined($length);
            exit 0 if length($data) >= $header_end + 4 + $length;
          }
          exit 1;
        ' >"$response_file"
      local -a pipeline_status=("${PIPESTATUS[@]}")
      set -e
      local nc_status="${pipeline_status[1]}"
      local parser_status="${pipeline_status[2]}"
      rtsp_nc_raw_status="$nc_status"
      if ((parser_status == 0)); then
        status=0
      else
        status="$nc_status"
        if ((status == 0)); then
          status="$parser_status"
        fi
        failures=$((failures + 1))
      fi
      {
        printf '\n=== rtsp-%s ===\n' "$label"
        printf 'request_uri=%s\n' "$rtsp_uri"
        printf 'request_bytes:\n'
        cat "$request_file"
        printf '\nresponse_bytes:\n'
        cat "$response_file"
        printf '\n\nnc_exit=%s\nnc_raw_exit=%s\n' \
          "$status" "$rtsp_nc_raw_status"
      } >>"$log_file"
    }

    probe_rtsp options OPTIONS
    probe_rtsp describe DESCRIBE

    video_file="${log_file%.*}.rtsp.ts"
    {
      printf '\n=== rtsp-video-capture ===\n'
      printf 'file=%s\n' "$video_file"
      printf 'duration_seconds=%s\n' "$rtsp_capture_seconds"
      printf 'idle_timeout_seconds=%s\n' "$rtsp_stream_timeout_seconds"
      printf 'transport=tcp\n'
    } >>"$log_file"

    if [[ -e "$video_file" ]]; then
      printf 'result=refusing-to-overwrite-existing-file\n' >>"$log_file"
      failures=$((failures + 1))
    elif ! command -v ffmpeg >/dev/null; then
      printf 'result=ffmpeg-required\n' >>"$log_file"
      failures=$((failures + 1))
    else
      video_log="$scratch_dir/ffmpeg-video.log"
      stream_timeout_microseconds=$((10#$rtsp_stream_timeout_seconds * 1000000))
      set +e
      ffmpeg \
        -hide_banner \
        -loglevel warning \
        -rtsp_transport tcp \
        -timeout "$stream_timeout_microseconds" \
        -i "$rtsp_uri" \
        -t "$rtsp_capture_seconds" \
        -map 0 \
        -c copy \
        -f mpegts \
        "$video_file" >"$video_log" 2>&1
      ffmpeg_status=$?
      set -e
      cat "$video_log" >>"$log_file"
      if [[ -e "$video_file" ]]; then
        video_size="$(wc -c <"$video_file" | tr -d ' ')"
      else
        video_size=0
      fi
      printf 'ffmpeg_exit=%s\nbytes=%s\n' "$ffmpeg_status" "$video_size" >>"$log_file"
      if ((ffmpeg_status != 0 || video_size == 0)); then
        failures=$((failures + 1))
      elif command -v ffprobe >/dev/null; then
        printf 'ffprobe:\n' >>"$log_file"
        ffprobe \
          -v error \
          -show_entries stream=index,codec_type,codec_name,width,height,avg_frame_rate \
          -of default=noprint_wrappers=1 \
          "$video_file" >>"$log_file" 2>&1 || true
      fi
    fi
  fi
fi

echo "Wrote $log_file"
if ((failures)); then
  echo "$failures probe(s) failed; the log contains the captured errors" >&2
  exit 1
fi
