#!/usr/bin/env python3
"""Exercise the CLI through a real Unix controlling terminal, keeping JSON separate."""
import errno
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import subprocess
import sys
import tempfile
import termios
import time


class Terminal:
    def __init__(self, binary, *args):
        master, slave = pty.openpty()
        self.master = master
        self.text = b""
        self.json_buffer = b""
        self.rows = []

        def session():
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)

        self.child = subprocess.Popen(
            [binary, *args, ":memory:"], stdin=slave, stdout=subprocess.PIPE,
            stderr=slave, preexec_fn=session, env={**os.environ, "TERM": "xterm"},
        )
        os.close(slave)
        try:
            self.wait(lambda: b"fastdb> " in self.text)
        except BaseException:
            self.cleanup()
            raise

    def wait(self, predicate):
        deadline = time.monotonic() + 10
        while not predicate():
            if time.monotonic() > deadline:
                raise AssertionError(("terminal timeout", self.text, self.rows))
            ready, _, _ = select.select([self.master, self.child.stdout], [], [], 0.1)
            for stream in ready:
                try:
                    data = os.read(stream if isinstance(stream, int) else stream.fileno(), 65536)
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
                    data = b""
                if stream == self.master:
                    self.text += data
                    if b"\x1b[6n" in data:
                        os.write(self.master, b"\x1b[1;1R")
                else:
                    self.json_buffer += data
                    while b"\n" in self.json_buffer:
                        line, self.json_buffer = self.json_buffer.split(b"\n", 1)
                        self.rows.append(json.loads(line))
            if self.child.poll() is not None and not predicate():
                raise AssertionError(("unexpected CLI exit", self.child.returncode, self.text, self.rows))

    def send(self, data, rows=None, prompt=None):
        self.text = b""
        os.write(self.master, data)
        self.wait(lambda: (rows is None or len(self.rows) >= rows)
                  and (prompt is None or prompt in self.text))

    def close(self):
        if self.child.poll() is None:
            os.write(self.master, b".quit\n")
        status = self.child.wait(timeout=10)
        assert status == 0, (status, self.text, self.rows)
        self.child.stdout.close()
        os.close(self.master)

    def cleanup(self):
        if self.child.poll() is None:
            os.killpg(self.child.pid, signal.SIGKILL)
            self.child.wait(timeout=10)
        self.child.stdout.close()
        try:
            os.close(self.master)
        except OSError:
            pass


def main():
    binary = str(Path(sys.argv[1]).resolve())
    with tempfile.TemporaryDirectory(prefix="fastdb-terminal-") as directory:
        history = Path(directory) / "history"
        terminal = Terminal(binary, "--history", str(history))
        try:
            terminal.send(b"SELECT 1;\n", rows=1, prompt=b"fastdb> ")
            terminal.send(b"\x1b[A\n", rows=2, prompt=b"fastdb> ")
            assert terminal.rows[1]["rows"][0][0]["value"] == 1
            terminal.send(b"SELECT 2x;\x1b[D\x7f\n", rows=3, prompt=b"fastdb> ")
            assert terminal.rows[2]["rows"][0][0]["value"] == 2
            terminal.send(b"SELECT\n", prompt=b"...> ")
            terminal.send(b"3;\n", rows=4, prompt=b"fastdb> ")
            terminal.send(b"\x1b[A\n", rows=5, prompt=b"fastdb> ")
            assert terminal.rows[4]["rows"][0][0]["value"] == 3
            terminal.send(b"BEGIN;\n", rows=6, prompt=b"fastdb(tx)> ")
            terminal.send(b"SELECT 'unfinished\n", prompt=b"...> ")
            terminal.send(b"\x03", prompt=b"fastdb(tx)> ")
            terminal.send(b"SELECT 4;\n", rows=7, prompt=b"fastdb(tx)> ")
            assert terminal.rows[6]["transaction"]["before"] == "active"
            assert terminal.rows[6]["rows"][0][0]["value"] == 4
            terminal.send(b"ROLLBACK;\n", rows=8, prompt=b"fastdb> ")
            terminal.send(b"SELECT 7;\n", rows=9, prompt=b"fastdb> ")
            terminal.send(b" SELECT 99;\n", rows=10, prompt=b"fastdb> ")
            terminal.close()
            contents = history.read_text()
            assert "unfinished" not in contents and "99" not in contents
            assert history.stat().st_mode & 0o077 == 0
        finally:
            terminal.cleanup()
        terminal = Terminal(binary, "--history", str(history))
        try:
            terminal.send(b"\x1b[A\n", rows=1, prompt=b"fastdb> ")
            assert terminal.rows[0]["rows"][0][0]["value"] == 7
            terminal.close()
        finally:
            terminal.cleanup()
        terminal = Terminal(binary, "--max-input-bytes", "8")
        try:
            terminal.send(b"SELECT 1234;\n", rows=1)
            assert terminal.rows[0]["error"]["code"] == "FDB_LIMIT"
            assert terminal.child.wait(timeout=10) == 1
        finally:
            terminal.cleanup()
        terminal = Terminal(binary)
        try:
            terminal.send(b"\x04")
            assert terminal.child.wait(timeout=10) == 0
        finally:
            terminal.cleanup()
    print("CLI terminal editing, history, prompt interruption and JSON isolation passed")


if __name__ == "__main__":
    main()
