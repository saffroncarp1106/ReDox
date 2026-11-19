"""Loopback Ollama API forwarding with a pre-inference callback."""

import http.client
from http.server import BaseHTTPRequestHandler
import json
from urllib.parse import urlsplit


INFERENCE = {
    "/api/chat", "/api/generate", "/api/embed", "/api/embeddings",
    "/v1/chat/completions", "/v1/completions", "/v1/responses", "/v1/embeddings",
    "/v1/messages", "/_redox/qualify",
}
HOP_HEADERS = {
    "connection", "keep-alive", "proxy-connection", "proxy-authenticate",
    "proxy-authorization", "te", "trailer", "transfer-encoding", "upgrade",
}
MAX_BODY = 64 * 1024 * 1024


def forward_headers(headers):
    fields = list(headers)
    connection = {token.strip().lower() for key, value in fields if key.lower() == "connection"
                  for token in value.split(",")}
    return [(key, value) for key, value in fields if key.lower() not in HOP_HEADERS | connection]


def make_handler(upstream, inference):
    target = urlsplit(upstream)

    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def json_response(self, status, value):
            body = json.dumps(value).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            if self.command != "HEAD":
                self.wfile.write(body)

        def read_body(self):
            transfer = self.headers.get("Transfer-Encoding", "").lower()
            if transfer == "chunked":
                data = bytearray()
                while True:
                    size = int(self.rfile.readline(256).split(b";", 1)[0].strip(), 16)
                    if size < 0 or len(data) + size > MAX_BODY:
                        raise ValueError("Ollama request body is too large.")
                    if size == 0:
                        return bytes(data)
                    data.extend(self.rfile.read(size))
                    if self.rfile.read(2) != b"\r\n":
                        raise ValueError("Invalid chunked Ollama request.")
            if transfer:
                raise ValueError("Unsupported request transfer encoding.")
            length = int(self.headers.get("Content-Length", 0))
            if not 0 <= length <= MAX_BODY:
                raise ValueError("Ollama request body is too large.")
            return self.rfile.read(length) if length else b""

        def dispatch(self):
            self.close_connection = True
            self.sent_response = False
            try:
                url = urlsplit(self.path)
                if self.command == "GET" and url.path == "/_redox/health":
                    self.json_response(200, {"service": "redox-qualified-ollama"})
                    return
                data = self.read_body()
                destination = url.path + (f"?{url.query}" if url.query else "")
                if self.command == "POST" and url.path in INFERENCE:
                    body = json.loads(data) if data else {}
                    if not isinstance(body, dict):
                        raise ValueError("Ollama request must be a JSON object.")
                    with inference(url.path, body) as prepared:
                        if url.path == "/_redox/qualify":
                            self.json_response(200, {"qualified": True, "model": prepared.get("model")})
                        else:
                            self.forward(destination, json.dumps(prepared).encode("utf-8"))
                else:
                    self.forward(destination, data)
            except (BrokenPipeError, ConnectionResetError):
                return
            except (ValueError, RuntimeError, OSError, http.client.HTTPException) as error:
                if not self.sent_response:
                    self.json_response(400 if isinstance(error, ValueError) else 502, {"error": str(error)})

        def forward(self, destination, body):
            conn = http.client.HTTPConnection(target.hostname, target.port, timeout=900)
            try:
                headers = {key: value for key, value in forward_headers(self.headers.items())
                           if key.lower() not in {"host", "content-length", "expect"}}
                conn.request(self.command, destination, body or None, headers)
                with conn.getresponse() as response:
                    self.sent_response = True
                    self.send_response(response.status, response.reason)
                    for key, value in forward_headers(response.getheaders()):
                        self.send_header(key, value)
                    self.send_header("Connection", "close")
                    self.end_headers()
                    if self.command != "HEAD":
                        while chunk := response.read1(65536):
                            self.wfile.write(chunk)
                            self.wfile.flush()
            finally:
                conn.close()

        do_GET = do_HEAD = do_POST = do_PUT = do_DELETE = do_OPTIONS = dispatch

    return Handler
