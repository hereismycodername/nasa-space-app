import contextlib
import http.server
import io
import json
from pathlib import Path
import signal
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import unittest

import sensor_bridge as bridge

SCRIPT = str(Path(bridge.__file__).resolve())
SAMPLE = {"metric": "temperature", "value": 36.6, "unit": "°C"}


@contextlib.contextmanager
def fake_api(statuses=()):
    requests = []
    remaining = list(statuses)

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            body = self.rfile.read(int(self.headers["Content-Length"]))
            requests.append((self.path, body))
            status = remaining.pop(0) if remaining else 201
            if status == 'truncated':
                self.send_response(201)
                self.send_header("Content-Length", "1000")
                self.end_headers()
                self.wfile.write(b'{"id":')
                self.close_connection = True
                return
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps({**json.loads(body), "id": "test-id", "source": "sensor"}).encode())

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}", requests
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


class BridgeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="space-health-bridge-test-")
        self.path = Path(self.temp.name) / "outbox.db"

    def tearDown(self):
        self.temp.cleanup()

    def command(self, url):
        return [sys.executable, SCRIPT, "--device-id", "test-sensor", "--outbox", str(self.path),
                "--api-url", url, "--retry-seconds", "0.05", "--request-timeout", "0.3"]

    def test_unknown_time_is_preserved_and_input_is_validated(self):
        self.assertIsNone(bridge.validate_sample(json.dumps(SAMPLE).encode())["measuredAt"])
        known = {**SAMPLE, "measuredAt": "2026-11-14T09:10:11.123456789+05:00"}
        self.assertEqual(bridge.validate_sample(json.dumps(known).encode()), known)
        large = bridge.validate_sample(json.dumps({**SAMPLE, "value": 9007199254740993}).encode())
        self.assertEqual(large["value"], 9007199254740992.0)
        bad = [b'{"metric":"mood","metric":"heartRate","value":2,"unit":"bpm"}',
               json.dumps({**SAMPLE, "value": True}).encode(),
               json.dumps({**SAMPLE, "value": float("nan")}).encode(),
               json.dumps({**SAMPLE, "unit": "ADC"}).encode(),
               json.dumps({**SAMPLE, "source": "sensor"}).encode(),
               json.dumps({**SAMPLE, "measuredAt": "2026-13-01T00:00:00Z"}).encode(),
               b"x" * 4097]
        for payload in bad:
            with self.subTest(payload=payload[:100]), self.assertRaises(bridge.BridgeError):
                bridge.validate_sample(payload)

    def test_api_origin_and_partial_serial_lines(self):
        self.assertEqual(bridge.api_origin("http://localhost:8080/api/"), "http://localhost:8080")
        for url in ["localhost:8080", "http://host/api/measurements", "http://host?x=1"]:
            with self.assertRaises(bridge.BridgeError):
                bridge.api_origin(url)

        class Serial:
            def __init__(self):
                self.parts = iter([b'{"metric":', b'', b'"mood","value":4,"unit":"score"}\n'])
            def readline(self, _size):
                return next(self.parts)

        line = next(bridge.input_lines(Serial(), serial=True))
        self.assertEqual(bridge.validate_sample(line)["metric"], "mood")
        with self.assertRaises(bridge.BridgeError):
            list(bridge.input_lines(io.BytesIO(b"x" * 4097)))

    def test_durable_identity_and_bounded_outbox(self):
        first = bridge.Outbox(self.path, "test-sensor")
        self.assertEqual(first.enqueue(bridge.validate_sample(json.dumps(SAMPLE).encode())), 0)
        pending = first.pending()
        with self.assertRaises(bridge.BridgeError):
            first.enqueue(SAMPLE)
        first.close()
        second = bridge.Outbox(self.path, "test-sensor")
        self.assertEqual(second.pending(), pending)
        second.acknowledge(0)
        self.assertEqual(second.enqueue(SAMPLE), 1)
        second.close()
        with self.assertRaises(bridge.BridgeError):
            bridge.Outbox(self.path, "different-device")

    def test_stdin_retry_and_finite_exit(self):
        with fake_api([503, 201]) as (url, requests):
            result = subprocess.run(self.command(url + "/api/"), input=json.dumps(SAMPLE) + "\n", text=True, capture_output=True, timeout=5)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(requests), 2)
        self.assertEqual(requests[0], requests[1])
        self.assertEqual(requests[0][0], "/api/measurements")
        self.assertIsNone(json.loads(requests[0][1])["measuredAt"])
        with sqlite3.connect(self.path) as db:
            self.assertEqual(db.execute("SELECT count(*) FROM pending").fetchone()[0], 0)

    def test_terminal_error_keeps_payload_for_restart(self):
        with fake_api([400]) as (url, requests):
            failed = subprocess.run(self.command(url), input=json.dumps(SAMPLE) + "\n", text=True, capture_output=True, timeout=5)
            resumed = subprocess.run(self.command(url) + ["--drain-only"], text=True, capture_output=True, timeout=5)
        self.assertEqual(failed.returncode, 1)
        self.assertIn("sample retained", failed.stderr)
        self.assertEqual(resumed.returncode, 0, resumed.stderr)
        self.assertEqual(requests[0][1], requests[1][1])

    def test_truncated_success_retries_identical_payload(self):
        with fake_api(['truncated', 200]) as (url, requests):
            result = subprocess.run(self.command(url), input=json.dumps(SAMPLE) + "\n", text=True, capture_output=True, timeout=5)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(requests), 2)
        self.assertEqual(requests[0][1], requests[1][1])
        self.assertIn("Delivery unconfirmed", result.stderr)

    def test_sigterm_during_retry_retains_payload_and_stops_promptly(self):
        with fake_api([503] * 100) as (url, requests):
            command = self.command(url) + ["--retry-seconds", "60"]
            process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            try:
                process.stdin.write(json.dumps(SAMPLE) + "\n")
                process.stdin.flush()
                deadline = time.monotonic() + 3
                while not requests and time.monotonic() < deadline:
                    time.sleep(0.01)
                self.assertTrue(requests)
                process.send_signal(signal.SIGTERM)
                _, stderr = process.communicate(timeout=1)
                self.assertEqual(process.returncode, 0, stderr)
                with sqlite3.connect(self.path) as db:
                    pending = db.execute("SELECT payload FROM pending").fetchone()[0]
                self.assertEqual(pending.encode(), requests[0][1])
            finally:
                if process.poll() is None:
                    process.kill()
                    process.communicate()


if __name__ == "__main__":
    unittest.main()
