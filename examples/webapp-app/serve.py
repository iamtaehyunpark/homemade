import http.server, socketserver, sys
class H(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        body = b"<html><body><h1 id='hi'>webapp ok</h1></body></html>"
        self.send_response(200); self.send_header("Content-Type", "text/html"); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
    def log_message(self, *a): pass
with socketserver.TCPServer(("127.0.0.1", 0), H) as srv:
    print(f"@homemade:port {srv.server_address[1]}", flush=True)
    srv.serve_forever()
