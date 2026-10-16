#!/usr/bin/env python3
"""Local-only browser fixture for the production alpha signup UI."""

import json
import mimetypes
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parent
PORT = int(os.environ.get("ALPHA_SIGNUP_PREVIEW_PORT", "4187"))
STATUS = int(os.environ.get("ALPHA_SIGNUP_PREVIEW_STATUS", "202"))
if STATUS not in (202, 422, 429, 503):
    raise SystemExit("ALPHA_SIGNUP_PREVIEW_STATUS must be 202, 422, 429, or 503")

TEST_SITE_KEY = "1x00000000000000000000AA"
TURNSTILE_MOCK = """<script>
window.__alphaSignupPreviewStatus = __PREVIEW_STATUS__;
window.fetch = ((realFetch) => (input, init = {}) => {
  const url = typeof input === 'string' ? input : input.url;
  if (!['/v1/alpha-signup/requests', '/v1/alpha-signup-removals'].includes(new URL(url, window.location.href).pathname)) {
    return realFetch(input, init);
  }
  const headers = new Headers(init.headers || (input instanceof Request ? input.headers : undefined));
  headers.set('X-Alpha-Signup-Preview-Status', String(window.__alphaSignupPreviewStatus));
  return realFetch(input, { ...init, headers });
})(window.fetch.bind(window));
document.addEventListener('DOMContentLoaded', () => {
  const panel = document.createElement('label');
  panel.className = 'alpha-preview-status';
  Object.assign(panel.style, {
    position: 'fixed', right: '12px', bottom: '12px', zIndex: '10000',
    display: 'grid', gap: '4px', padding: '10px 12px', border: '1px solid #8a7a63',
    borderRadius: '8px', background: '#fffdf8', color: '#26231f',
    font: '12px/1.4 system-ui, sans-serif', boxShadow: '0 2px 12px #0002'
  });
  panel.innerHTML = 'Local response <select aria-label="Local preview response status">'
    + '<option value="202">202 received</option><option value="422">422 validation</option>'
    + '<option value="429">429 rate limit</option><option value="503">503 unavailable</option>'
    + '</select>';
  panel.querySelector('select').addEventListener('change', (event) => {
    window.__alphaSignupPreviewStatus = Number(event.target.value);
  });
  document.body.append(panel);
});
window.turnstile = {
  render(selector, options) {
    window.__alphaSignupPreviewWidget = options;
    const mount = document.querySelector(selector);
    if (mount) mount.innerHTML = '<div class="preview-turnstile">Local test fixture · Turnstile is mocked</div>';
    setTimeout(() => options.callback('local-mock-token'), 20);
    return 'local-preview-widget';
  },
  reset() {
    const options = window.__alphaSignupPreviewWidget;
    if (options) setTimeout(() => options.callback('local-mock-token'), 20);
  },
  remove() { window.__alphaSignupPreviewWidget = null; }
};
</script>
"""


class Handler(BaseHTTPRequestHandler):
    def log_message(self, _format, *_args):
        # Avoid access logs that could include request URLs or submitted data.
        return

    def send_body(self, status, content_type, body):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        path = unquote(urlsplit(self.path).path)
        if path == "/v1/alpha-signup/config":
            body = json.dumps({"enabled": True, "site_key": TEST_SITE_KEY, "action": "alpha_signup"}).encode()
            self.send_body(200, "application/json; charset=utf-8", body)
            return

        relative = "index.html" if path == "/" else path.lstrip("/")
        target = (ROOT / relative).resolve()
        if not target.is_relative_to(ROOT) or not target.is_file():
            self.send_body(404, "text/plain; charset=utf-8", b"Not found")
            return
        content = target.read_bytes()
        if relative == "index.html":
            marker = b'    <script src="site.js?'
            content = content.replace(marker, TURNSTILE_MOCK.replace("__PREVIEW_STATUS__", str(STATUS)).encode() + marker, 1)
        content_type = mimetypes.guess_type(target.name)[0] or "application/octet-stream"
        if content_type in ("text/html", "text/css", "text/javascript", "application/javascript"):
            content_type += "; charset=utf-8"
        self.send_body(200, content_type, content)

    def do_POST(self):
        path = urlsplit(self.path).path
        if path not in ("/v1/alpha-signup/requests", "/v1/alpha-signup-removals"):
            self.send_body(404, "application/json", b"{}")
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
            payload = json.loads(self.rfile.read(min(length, 32768)))
        except (ValueError, json.JSONDecodeError):
            self.send_body(422, "application/json", b"{}")
            return
        expected = (
            {"idempotency_key", "email", "turnstile_token", "website"}
            if path == "/v1/alpha-signup-removals"
            else {
                "idempotency_key", "email", "pev_type", "pev_model",
                "requested_accessories", "locale", "turnstile_token", "website",
            }
        )
        if not isinstance(payload, dict) or set(payload) != expected:
            self.send_body(422, "application/json", b"{}")
            return
        try:
            requested_status = int(self.headers.get("X-Alpha-Signup-Preview-Status", str(STATUS)))
        except ValueError:
            requested_status = STATUS
        if requested_status not in (202, 422, 429, 503):
            requested_status = STATUS
        if requested_status == 202:
            self.send_body(202, "application/json; charset=utf-8", b'{"status":"received","request_id":"local-preview"}')
        else:
            self.send_body(requested_status, "application/json; charset=utf-8", b"{}")


if __name__ == "__main__":
    server = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    print(f"Local alpha signup preview: http://127.0.0.1:{PORT}/?page=signup&lang=en")
    print("No form data is saved or printed. Stop with Ctrl-C.")
    server.serve_forever()
