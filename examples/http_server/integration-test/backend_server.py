#!/usr/bin/env python3
"""
Simple HTTP backend server for load balancing tests.
Each instance returns its own ID to verify load balancing distribution.
"""

import http.server
import json
import os
import socketserver
import time
from urllib.parse import urlparse, parse_qs

# Get server ID from environment (set by docker-compose)
SERVER_ID = os.environ.get("SERVER_ID", "unknown")
PORT = int(os.environ.get("PORT", "8000"))

# Track request count
request_count = 0


class BackendHandler(http.server.BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        # Quieter logging
        pass

    def do_GET(self):
        global request_count
        request_count += 1
        
        parsed = urlparse(self.path)
        
        if parsed.path == "/health":
            # Health check endpoint
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            response = {
                "status": "healthy",
                "server_id": SERVER_ID,
            }
            self.wfile.write(json.dumps(response).encode())
            
        elif parsed.path == "/api/echo":
            # Echo endpoint for testing
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            response = {
                "server_id": SERVER_ID,
                "request_count": request_count,
                "path": self.path,
                "timestamp": time.time(),
            }
            self.wfile.write(json.dumps(response).encode())
            
        elif parsed.path == "/api/slow":
            # Slow endpoint for testing timeouts
            delay = float(parse_qs(parsed.query).get("delay", ["1"])[0])
            time.sleep(delay)
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            response = {
                "server_id": SERVER_ID,
                "delay": delay,
            }
            self.wfile.write(json.dumps(response).encode())
            
        elif parsed.path.startswith("/api/"):
            # Generic API response
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            response = {
                "server_id": SERVER_ID,
                "request_count": request_count,
                "path": parsed.path,
            }
            self.wfile.write(json.dumps(response).encode())
            
        else:
            self.send_response(404)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps({"error": "not found"}).encode())

    def do_POST(self):
        self.do_GET()


if __name__ == "__main__":
    with socketserver.TCPServer(("", PORT), BackendHandler) as httpd:
        print(f"Backend server {SERVER_ID} listening on port {PORT}")
        httpd.serve_forever()

