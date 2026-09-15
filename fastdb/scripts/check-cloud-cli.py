#!/usr/bin/env python3
"""Generic cloud HTTP client contract; only synthetic credentials and data."""
import http.server
import json
import os
import subprocess
import sys
import threading
import uuid

binary = sys.argv[1]
key = 'fdbk_' + 'a' * 64
database = str(uuid.uuid4())
requests = []
mode = 'normal'
query_attempts = []

class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def handle_request(self):
        assert self.headers.get('authorization') == f'Bearer {key}'
        body = self.rfile.read(int(self.headers.get('content-length', 0)))
        value = json.loads(body) if body else None
        requests.append((self.command, self.path, value))
        status = 200
        result = {'id': database, 'sequence': 7, 'url': 'https://untrusted.example/query'}
        if mode == 'large':
            result = {'data': 'x' * (600 * 1024)}
        elif mode == 'redirect':
            status = 307
            result = {'error': f'echo {key}'}
        elif self.path == '/v1/whoami':
            result = {'user': {'email': 'synthetic@example.test'}, 'echo': key}
        elif self.path.endswith('/query'):
            query_attempts.append(value)
            if mode == 'retry' and len(query_attempts) == 1:
                status, result = 503, {'error': 'not acknowledged'}
            elif mode == 'unresolved':
                status, result = (503 if len(query_attempts) == 1 else 409), {'error': 'unknown outcome'}
            else:
                result = {'sequence': 8, 'results': []}
        self.send_response(status)
        if status == 307:
            self.send_header('Location', 'https://untrusted.example/')
        self.send_header('Content-Type', 'application/json')
        encoded = json.dumps(result).encode()
        self.send_header('Content-Length', str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    do_GET = handle_request
    do_POST = handle_request
    do_DELETE = handle_request

server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
thread = threading.Thread(target=server.serve_forever, daemon=True)
thread.start()
env = {**os.environ, 'FASTDB_API_KEY': key, 'FASTDB_CLOUD_URL': f'http://127.0.0.1:{server.server_port}'}
def run(args, sql='', success=True, settings=None):
    result = subprocess.run([binary, 'cloud', *args], input=sql, text=True, capture_output=True,
                            env=settings or env, timeout=20)
    assert (result.returncode == 0) == success, (args, result.returncode, result.stderr)
    assert key not in result.stdout + result.stderr, 'credential printed in diagnostics/output'
    return result
try:
    assert 'whoami' in run(['--help']).stdout
    assert '[redacted]' in run(['whoami']).stdout
    for args, method, path in [
        (['db', 'list'], 'GET', '/v1/databases'),
        (['db', 'create', 'example'], 'POST', '/v1/databases'),
        (['db', 'show', database], 'GET', f'/v1/databases/{database}'),
        (['db', 'delete', database], 'DELETE', f'/v1/databases/{database}'),
    ]:
        run(args)
        assert requests[-1][:2] == (method, path)
    run(['db', 'access', database], "SELECT 1;\nSELECT\n2;\n.quit\n")
    assert len(query_attempts) == 2
    assert all(q['expectedSequence'] == 7 for q in query_attempts)
    assert len({q['requestId'] for q in query_attempts}) == 2
    run(['db', 'access', database], 'SELECT 1; SELECT 2;\n.quit\n')
    assert len(query_attempts[-1]['statements']) == 2
    query_attempts.clear()
    mode = 'retry'
    result = run(['db', 'access', database], 'INSERT INTO t VALUES (1);\n.retry\n.quit\n', success=False)
    assert len(query_attempts) == 2 and query_attempts[0] == query_attempts[1]
    assert json.loads(result.stdout.splitlines()[-1])['sequence'] == 8
    query_attempts.clear()
    mode = 'unresolved'
    run(['db', 'access', database], 'INSERT INTO t VALUES (2);\n.retry\nINSERT INTO t VALUES (3);\n.quit\n', success=False)
    assert len(query_attempts) == 2 and query_attempts[0] == query_attempts[1]
    mode = 'redirect'
    before = len(requests)
    run(['whoami'], success=False)
    assert len(requests) == before + 1
    mode = 'large'
    run(['whoami'], success=False)
    for endpoint in ['http://example.test', 'https://user:pass@example.test', 'https://example.test/?key=bad']:
        run(['whoami'], success=False, settings={**env, 'FASTDB_CLOUD_URL': endpoint})
    run(['db', 'show', '../metadata'], success=False)
    print('Cloud CLI management, interactive batching, stable retries, redaction and endpoint checks passed')
finally:
    server.shutdown()
    server.server_close()
