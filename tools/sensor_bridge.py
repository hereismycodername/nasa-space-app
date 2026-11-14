#!/usr/bin/env python3
"""Bounded NDJSON -> durable SQLite outbox -> Space Health HTTP ingestion.

Only optional serial input needs pyserial. stdin and HTTP use the Python standard
library. One unconfirmed sample blocks further input; its identity and payload
survive retries and process restarts. This is not a board-specific sensor driver.
"""
import argparse
import contextlib
import datetime as dt
import http.client
import json
import math
import os
from pathlib import Path
import re
import signal
import sqlite3
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid

MAX_LINE = 4096
UNITS = {"heartRate": "bpm", "systolicPressure": "mmHg", "diastolicPressure": "mmHg",
         "temperature": "°C", "sleepHours": "h", "mood": "score"}


class BridgeError(Exception):
    """Actionable error; any unconfirmed sample remains in the outbox."""


def api_origin(value):
    try:
        url = urllib.parse.urlsplit(value)
        port = url.port
    except ValueError as error:
        raise BridgeError("Invalid --api-url") from error
    if (url.scheme not in ("http", "https") or not url.hostname or url.username or url.password
            or url.query or url.fragment or url.path not in ("", "/", "/api", "/api/")):
        raise BridgeError("--api-url must be an http(s) origin, optionally ending in /api; no credentials, query or other path")
    if port is not None and not 1 <= port <= 65535:
        raise BridgeError("Invalid API port")
    return urllib.parse.urlunsplit((url.scheme, url.netloc, "", "", ""))


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise BridgeError(f"Duplicate JSON field: {key}")
        result[key] = value
    return result


def reject_constant(_value):
    raise BridgeError("Non-finite JSON number")


def validate_sample(raw):
    if len(raw) > MAX_LINE:
        raise BridgeError(f"Input line exceeds {MAX_LINE} bytes")
    try:
        value = json.loads(raw.decode("utf-8"), object_pairs_hook=unique_object, parse_constant=reject_constant)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise BridgeError(f"Expected one UTF-8 JSON measurement per line: {error}") from error
    if not isinstance(value, dict) or not {"metric", "value", "unit"} <= value.keys():
        raise BridgeError("Each line requires metric, value and unit")
    if value.keys() - {"metric", "value", "unit", "measuredAt"}:
        raise BridgeError("Only metric, value, unit and optional measuredAt are accepted; identity belongs to the bridge")
    metric = value["metric"]
    if not isinstance(metric, str) or metric not in UNITS:
        raise BridgeError("Unknown metric")
    if value["unit"] != UNITS[metric]:
        raise BridgeError(f"{metric} requires unit {UNITS[metric]}")
    number = value["value"]
    try:
        finite = isinstance(number, (int, float)) and not isinstance(number, bool) and math.isfinite(number)
    except OverflowError:
        finite = False
    if not finite or not (0 <= number <= 24 if metric == "sleepHours" else 1 <= number <= 5 if metric == "mood" else number > 0):
        raise BridgeError("Value is outside the API input bounds (these are not medical thresholds)")
    number = float(number)  # Match the API's f64 representation before persisting retry bytes.
    timestamp = value.get("measuredAt")
    if timestamp is not None:
        # Preserve the original text and nanoseconds. Server remains authoritative
        # for its clock and the +300s ingestion limit; never use receipt time here.
        if not isinstance(timestamp, str) or not re.fullmatch(r"\d{4}-\d\d-\d\d[Tt]\d\d:\d\d:\d\d(?:\.\d+)?(?:[Zz]|[+-]\d\d:\d\d)", timestamp):
            raise BridgeError("measuredAt must be RFC3339 with a timezone or null")
        try:
            # Python cannot represent leap seconds, but Chrono accepts them.
            ordinary = re.sub(r"(:)60(?=\.|[Zz]|[+-])", r"\g<1>59", timestamp)
            dt.datetime.fromisoformat(ordinary.replace("t", "T").replace("z", "+00:00").replace("Z", "+00:00"))
        except ValueError as error:
            raise BridgeError("Invalid measuredAt calendar/time") from error
    return {"metric": metric, "value": number, "unit": value["unit"], "measuredAt": timestamp}


@contextlib.contextmanager
def outbox_lock(path):
    """One bridge per outbox; avoids two readers consuming the same stdin/device."""
    handle = open(str(path) + ".lock", "a+b")
    try:
        if os.name == "nt":
            import msvcrt
            handle.seek(0)
            if not handle.read(1):
                handle.write(b"0")
                handle.flush()
            handle.seek(0)
            try:
                msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
            except OSError as error:
                raise BridgeError("Another bridge is already using this outbox") from error
        else:
            import fcntl
            try:
                fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            except OSError as error:
                raise BridgeError("Another bridge is already using this outbox") from error
        yield
    finally:
        handle.close()


class Outbox:
    def __init__(self, path, device):
        self.db = sqlite3.connect(path, timeout=5)
        self.db.execute("PRAGMA journal_mode=WAL")
        self.db.execute("PRAGMA synchronous=FULL")
        with self.db:
            self.db.execute("CREATE TABLE IF NOT EXISTS identity (singleton INTEGER PRIMARY KEY CHECK(singleton=1), device TEXT NOT NULL, session TEXT NOT NULL, next_sequence INTEGER NOT NULL)")
            self.db.execute("CREATE TABLE IF NOT EXISTS pending (sequence INTEGER PRIMARY KEY, payload TEXT NOT NULL)")
            self.db.execute("INSERT OR IGNORE INTO identity VALUES (1, ?, ?, 0)", (device, str(uuid.uuid4())))
        self.device, self.session, _ = self.db.execute("SELECT device, session, next_sequence FROM identity WHERE singleton=1").fetchone()
        if self.device != device:
            self.db.close()
            raise BridgeError("Outbox belongs to a different device; use that device-id or a separate outbox file")

    def close(self):
        self.db.close()

    def pending(self):
        return self.db.execute("SELECT sequence, payload FROM pending ORDER BY sequence LIMIT 1").fetchone()

    def enqueue(self, sample):
        with self.db:
            if self.pending():
                raise BridgeError("Outbox full: deliver the pending sample before reading more input")
            sequence = self.db.execute("SELECT next_sequence FROM identity WHERE singleton=1").fetchone()[0]
            if sequence >= 9223372036854775807:
                raise BridgeError("Sequence exhausted; drain and use a new outbox")
            payload = json.dumps({"deviceId": self.device, "sessionId": self.session,
                                  "sequence": sequence, **sample}, ensure_ascii=False, allow_nan=False, separators=(",", ":"))
            self.db.execute("INSERT INTO pending VALUES (?, ?)", (sequence, payload))
            self.db.execute("UPDATE identity SET next_sequence=? WHERE singleton=1", (sequence + 1,))
        return sequence

    def acknowledge(self, sequence):
        with self.db:
            self.db.execute("DELETE FROM pending WHERE sequence=?", (sequence,))


def deliver(outbox, base, timeout, retry):
    """Retry identical bytes until acknowledged. 4xx retains the row and stops."""
    while (pending := outbox.pending()) is not None:
        sequence, payload = pending
        request = urllib.request.Request(base + "/api/measurements", data=payload.encode("utf-8"),
                                         headers={"Content-Type": "application/json"}, method="POST")
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                if response.status not in (200, 201):
                    raise BridgeError(f"Unexpected success status {response.status}; sample retained")
                raw = response.read(65537)
                if len(raw) > 65536:
                    raise BridgeError("Unexpected oversized API response; sample retained")
                length = response.headers.get("Content-Length")
                if length is not None:
                    try:
                        complete = len(raw) == int(length)
                    except ValueError:
                        complete = False
                    if not complete:
                        raise http.client.IncompleteRead(raw)
                try:
                    returned = json.loads(raw)
                except (ValueError, UnicodeDecodeError) as error:
                    raise BridgeError("API response is not a measurement JSON; sample retained") from error
                expected = json.loads(payload)
                if not isinstance(returned, dict) or any(returned.get(key) != expected[key] for key in ("deviceId", "sessionId", "sequence", "metric", "unit", "value", "measuredAt")) or returned.get("source") != "sensor":
                    raise BridgeError("API did not confirm this sensor measurement; sample retained. Check registration and API URL.")
            outbox.acknowledge(sequence)
            print(f"Confirmed sensor sequence {sequence}", file=sys.stderr, flush=True)
        except urllib.error.HTTPError as error:
            if error.code < 500 and error.code != 429:
                try:
                    detail = error.read(1000).decode("utf-8", errors="replace")
                except (OSError, http.client.HTTPException):
                    detail = "response body unavailable"
                finally:
                    error.close()
                raise BridgeError(f"HTTP {error.code}: {detail}; sample retained. Register the device/check input, then restart with --drain-only.") from error
            error.close()
            print(f"HTTP {error.code}; retrying sequence {sequence} in {retry}s", file=sys.stderr, flush=True)
            time.sleep(retry)
        except (urllib.error.URLError, OSError, http.client.HTTPException) as error:
            print(f"Delivery unconfirmed ({error}); retrying SAME sequence {sequence} in {retry}s; input paused", file=sys.stderr, flush=True)
            time.sleep(retry)


def input_lines(stream, serial=False):
    buffered = bytearray()
    while True:
        part = stream.readline(MAX_LINE + 1 - len(buffered))
        if not part:
            if serial:
                continue  # timeout is not EOF; keep any partial frame
            if buffered:
                yield bytes(buffered)
            return
        buffered.extend(part)
        if len(buffered) > MAX_LINE:
            raise BridgeError(f"Input line exceeds {MAX_LINE} bytes; bridge stopped without enqueueing it")
        if buffered.endswith(b"\n"):
            if buffered.strip():
                yield bytes(buffered)
            buffered.clear()


def positive(value):
    result = float(value)
    if not math.isfinite(result) or not 0 < result <= 60:
        raise argparse.ArgumentTypeError("must be >0 and <=60 seconds")
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--device-id", required=True, help="registered sensor device; one outbox per device")
    parser.add_argument("--api-url", default=os.environ.get("API_URL", "http://127.0.0.1:8080"))
    parser.add_argument("--outbox", type=Path, default=Path("sensor-outbox.db"))
    parser.add_argument("--port", help="serial port, e.g. /dev/serial/by-id/...; omitted = stdin")
    parser.add_argument("--baud", type=int, default=115200)
    parser.add_argument("--rtscts", action="store_true", help="enable hardware flow control if supported/wired by your board")
    parser.add_argument("--drain-only", action="store_true", help="deliver saved pending sample, then exit without reading input")
    parser.add_argument("--request-timeout", type=positive, default=5)
    parser.add_argument("--retry-seconds", type=positive, default=2)
    options = parser.parse_args(argv)
    if not 1 <= len(options.device_id.encode("utf-8")) <= 128 or options.baud <= 0:
        parser.error("device-id must be 1..128 UTF-8 bytes; baud must be positive")
    outbox = None
    try:
        base = api_origin(options.api_url)
        if not options.outbox.parent.exists():
            raise BridgeError("Outbox parent directory does not exist; create a writable directory first")
        with outbox_lock(options.outbox):
            outbox = Outbox(options.outbox, options.device_id)
            print(f"Sensor bridge -> {base}; outbox {options.outbox}; input pauses until delivery is confirmed", file=sys.stderr, flush=True)
            deliver(outbox, base, options.request_timeout, options.retry_seconds)
            if options.drain_only:
                return 0
            with contextlib.ExitStack() as stack:
                if options.port:
                    try:
                        import serial
                    except ImportError as error:
                        raise BridgeError("Serial input needs pyserial: create a venv and pip install -r tools/requirements-serial.txt") from error
                    stream = stack.enter_context(serial.Serial(options.port, options.baud, timeout=0.2, rtscts=options.rtscts))
                else:
                    stream = sys.stdin.buffer
                for line in input_lines(stream, serial=bool(options.port)):
                    outbox.enqueue(validate_sample(line))
                    deliver(outbox, base, options.request_timeout, options.retry_seconds)
        return 0
    except KeyboardInterrupt:
        print("Bridge stopped; any unconfirmed measurement remains in the outbox. Restart with the same device-id/outbox.", file=sys.stderr)
        return 0
    except (BridgeError, OSError, sqlite3.Error) as error:
        print(f"Sensor bridge: {error}", file=sys.stderr)
        return 1
    finally:
        if outbox:
            outbox.close()


if __name__ == "__main__":
    def stop(_signal, _frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGINT, stop)
    signal.signal(signal.SIGTERM, stop)
    raise SystemExit(main())
