import http.server, socketserver, socket, time
class H(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        body = b"<html><body><h1 id='hi'>webapp ok</h1></body></html>"
        self.send_response(200); self.send_header("Content-Type", "text/html"); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
    def log_message(self, *a): pass
# Pick a free port, announce it, then deliberately delay binding: Homemade must show a loading state, not a blank iframe.
s = socket.socket(); s.bind(("127.0.0.1", 0)); port = s.getsockname()[1]; s.close()
print(f"@homemade:port {port}", flush=True)
time.sleep(2)
socketserver.TCPServer.allow_reuse_address = True
with socketserver.TCPServer(("127.0.0.1", port), H) as srv:
    srv.serve_forever()
