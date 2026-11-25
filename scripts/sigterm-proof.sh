#!/usr/bin/env bash
# Hidden packaged-app SIGTERM + startup stale-sending acceptance (no open -a, no focus steal).
# Usage:
#   bash scripts/sigterm-proof.sh                     # uses release .app if present, else debug/release bin
#   WIRE_APP="/path/to/Wire Studio KH.app" bash scripts/sigterm-proof.sh
#   WIRE_BIN="/path/to/wire-studio-kh" bash scripts/sigterm-proof.sh
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
default_app="$root/src-tauri/target/release/bundle/macos/Wire Studio KH.app"

if [ -n "${WIRE_APP:-}" ]; then
  target_kind="app"
  target_path="$WIRE_APP"
elif [ -z "${WIRE_BIN:-}" ] && [ -d "$default_app" ]; then
  target_kind="app"
  target_path="$default_app"
else
  target_kind="bin"
  target_path="${WIRE_BIN:-$root/src-tauri/target/debug/wire-studio-kh}"
  [ -x "$target_path" ] || target_path="$root/src-tauri/target/release/wire-studio-kh"
fi

exec python3 -I - "$target_kind" "$target_path" <<'PY'
import ctypes as ct
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import signal
import socket
import sqlite3
import subprocess as sp
import sys
import tempfile
import threading
import time
import urllib.request
from datetime import datetime, timedelta, timezone

KIND = sys.argv[1]
TARGET = Path(sys.argv[2]).resolve()
CONTRACT_MAX_S = 10.0
RACE_BOUND_S = 12.0


def command(*argv):
    return sp.check_output(argv, text=True).strip()


def foreground():
    return command(
        '/usr/bin/osascript',
        '-l',
        'JavaScript',
        '-e',
        'ObjC.import("AppKit"); String($.NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier)',
    )


def visible_windows(pid):
    cg = ct.CDLL('/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics')
    cf = ct.CDLL('/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation')
    ptr, index = ct.c_void_p, ct.c_long
    cg.CGWindowListCopyWindowInfo.argtypes = (ct.c_uint32, ct.c_uint32)
    cg.CGWindowListCopyWindowInfo.restype = ptr
    cf.CFArrayGetCount.argtypes, cf.CFArrayGetCount.restype = (ptr,), index
    cf.CFArrayGetValueAtIndex.argtypes, cf.CFArrayGetValueAtIndex.restype = (ptr, index), ptr
    cf.CFDictionaryGetValue.argtypes, cf.CFDictionaryGetValue.restype = (ptr, ptr), ptr
    cf.CFNumberGetValue.argtypes, cf.CFNumberGetValue.restype = (ptr, index, ptr), ct.c_bool
    cf.CFRelease.argtypes, cf.CFRelease.restype = (ptr,), None
    owner_key = ptr.in_dll(cg, 'kCGWindowOwnerPID')
    layer_key = ptr.in_dll(cg, 'kCGWindowLayer')
    rows = cg.CGWindowListCopyWindowInfo(17, 0)  # Onscreen only, excluding desktop elements.
    assert rows, 'Window Server did not return a window list'

    def number(row, key):
        value = ct.c_int()
        num = cf.CFDictionaryGetValue(row, key)
        return value.value if num and cf.CFNumberGetValue(num, 9, ct.byref(value)) else None

    try:
        windows = (cf.CFArrayGetValueAtIndex(rows, i) for i in range(cf.CFArrayGetCount(rows)))
        return sum(number(w, owner_key) == pid and number(w, layer_key) == 0 for w in windows)
    finally:
        cf.CFRelease(rows)


def processes():
    found = {}
    for row in command('/bin/ps', '-axo', 'pid=,ppid=,state=,command=').splitlines():
        parts = row.strip().split(None, 3)
        if len(parts) < 4:
            continue
        pid, ppid, state, cmd = parts
        if not state.startswith('Z'):
            found[int(pid)] = (int(ppid), cmd)
    return found


def children(root_pid, table):
    seen = {root_pid}
    pending = seen.copy()
    while pending:
        pending = {pid for pid, (parent, _) in table.items() if parent in pending} - seen
        seen.update(pending)
    return sorted(seen - {root_pid})


def hold_owners():
    return sp.run(
        ('/usr/sbin/lsof', '-nP', '-iTCP:8649', '-sTCP:LISTEN', '-t'),
        capture_output=True,
        text=True,
    ).stdout.split()


def until(test, seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        result = test()
        if result:
            return result
        time.sleep(0.05)
    return None


def post(url, payload, header, token):
    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode(),
        headers={'Content-Type': 'application/json', header: token},
    )
    with urllib.request.urlopen(req, timeout=5) as response:
        return json.load(response)


def hashes(path):
    result = {}
    for entry in path.iterdir():
        if entry.is_file():
            digest = hashlib.sha256()
            with entry.open('rb') as stream:
                for block in iter(lambda: stream.read(65536), b''):
                    digest.update(block)
            result[entry.name] = digest.hexdigest()
    return result


def now_z():
    return datetime.now(timezone.utc).isoformat(timespec='seconds').replace('+00:00', 'Z')


def row_tuple(data, delivery_id):
    with sqlite3.connect((data / 'wire.db').as_uri() + '?mode=ro', uri=True) as db:
        return db.execute(
            'SELECT state, attempts, claimed_at, last_error FROM deliveries WHERE id=?', (delivery_id,)
        ).fetchone()


class SlowProxy:
    """A local HTTPS proxy that accepts CONNECT and answers 502 after a delay.

    reqwest honours HTTPS_PROXY, so the real Telegram adapter is genuinely in flight while the
    proxy withholds its answer. Nothing is sent to Telegram and no certificate is trusted.
    """

    def __init__(self, delay_s):
        self.delay_s = delay_s
        self.port = None
        self.seen = []
        self.sock = None

    def start(self):
        self.sock = socket.socket()
        self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.sock.bind(('127.0.0.1', 0))
        self.sock.listen(8)
        self.port = self.sock.getsockname()[1]
        threading.Thread(target=self._accept, daemon=True).start()

    def _accept(self):
        while True:
            try:
                conn, _ = self.sock.accept()
            except OSError:
                return
            threading.Thread(target=self._hold, args=(conn,), daemon=True).start()

    def _hold(self, conn):
        try:
            conn.settimeout(10)
            head = conn.recv(4096)
            self.seen.append(head.split(b'\r\n', 1)[0].decode('ascii', 'replace'))
            time.sleep(self.delay_s)
            conn.sendall(b'HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n')
        except Exception:
            pass
        finally:
            conn.close()

    def stop(self):
        if self.sock is not None:
            self.sock.close()


def assert_free():
    ending = '/Wire Studio KH.app/Contents/MacOS/Wire Studio KH'
    live = [
        (pid, c)
        for pid, (_, c) in processes().items()
        if c.endswith(ending) or (ending + ' --') in c or 'wire-studio-kh --' in c
    ]
    assert not hold_owners() and not live, f'Wire or hold port 8649 already live: owners={hold_owners()} live={live}'


def prepare_target(root):
    if KIND == 'app':
        assert TARGET.is_dir(), f'App bundle not found: {TARGET}'
        app = root / TARGET.name
        sp.run(('/usr/bin/ditto', str(TARGET), str(app)), check=True)
        exe = app / 'Contents/MacOS/Wire Studio KH'
        assert exe.is_file() and os.access(exe, os.X_OK), f'Bundle executable missing: {exe}'
        version = command(
            '/usr/libexec/PlistBuddy',
            '-c',
            'Print CFBundleShortVersionString',
            str(app / 'Contents/Info.plist'),
        )
        return str(exe), f'packaged .app {version} (copied from {TARGET})'
    assert TARGET.is_file() and os.access(TARGET, os.X_OK), f'Binary not found or not executable: {TARGET}'
    return str(TARGET), f'binary ({TARGET})'


def run_hidden_cycle(exe, env, token, data, log_path, front, label, expect_hold_safe=True, expect_no_reclaim=False):
    proc = None
    with log_path.open('w+') as log:
        try:
            launched = time.monotonic()
            proc = sp.Popen(
                [exe, '--hidden'],
                env={**os.environ, **env, 'WIRE_STUDIO_KH_HIDDEN': '1'},
                stdout=log,
                stderr=sp.STDOUT,
            )
            pid = proc.pid
            assert until(lambda: str(pid) in hold_owners() or proc.poll() is not None, 8), (
                f'[{label}] process {pid} did not bind hold port 8649'
            )
            assert proc.poll() is None, f'[{label}] process exited early rc={proc.returncode}'
            assert str(pid) in hold_owners(), f'[{label}] hold port 8649 not owned by {pid}'

            hold = post(
                'http://127.0.0.1:8649/api/maintenance/hold?timeout_s=0&pause_s=25',
                {},
                'x-hold-token',
                token,
            )
            print(f'[{label}] Update hold before SIGTERM: {json.dumps(hold, sort_keys=True)}', flush=True)
            if expect_hold_safe:
                assert hold.get('safe') is True and hold.get('in_flight') == 0 and hold.get('unsettled') == [], (
                    f'[{label}] unexpected hold reply: {hold}'
                )

            time.sleep(max(0.0, 2.0 - (time.monotonic() - launched)))
            onscreen = visible_windows(pid)
            same_front = foreground() == front
            descendants = children(pid, processes())
            print(
                f'[{label}] Desktop safety before SIGTERM: PID={pid}; visible normal windows={onscreen}; '
                f'foreground unchanged={same_front}; descendants={descendants}',
                flush=True,
            )
            assert onscreen == 0 and same_front, (
                f'[{label}] Desktop safety failed: onscreen={onscreen}, same_front={same_front}'
            )

            before = hashes(data)
            begin = time.monotonic()
            os.kill(pid, signal.SIGTERM)
            watched = {pid, *descendants}
            gone = until(lambda: proc.poll() is not None and not (watched & processes().keys()), RACE_BOUND_S)
            elapsed = time.monotonic() - begin
            rc = proc.poll()
            survivors = sorted(watched & processes().keys())
            free = not hold_owners()
            same_front_after = foreground() == front
            print(
                f'[{label}] SIGTERM: exit_code={rc}; all_gone={bool(gone)} in {elapsed:.3f}s; '
                f'survivors={survivors}; hold_port_free={free}; foreground_unchanged={same_front_after}',
                flush=True,
            )

            after = hashes(data)
            changed = [
                name for name in sorted(before.keys() | after.keys()) if before.get(name) != after.get(name)
            ]
            no_hold_file = not any('hold' in name for name in after)
            print(
                f'[{label}] Data files changed during SIGTERM: {changed}; no persisted hold file={no_hold_file}',
                flush=True,
            )

            log.flush()
            log.seek(0)
            stderr_text = log.read()
            assert gone and not survivors, f'[{label}] process or descendants survived SIGTERM: {survivors}'
            assert rc == 0, f'[{label}] expected graceful exit 0 on SIGTERM, observed rc={rc}'
            assert elapsed <= CONTRACT_MAX_S, f'[{label}] SIGTERM took {elapsed:.3f}s (> {CONTRACT_MAX_S}s contract)'
            assert free, f'[{label}] hold port 8649 still busy after exit'
            assert same_front_after, f'[{label}] foreground changed during SIGTERM'
            assert no_hold_file, f'[{label}] unexpected hold file persisted in data dir'
            assert 'shutdown: signal=SIGTERM held=true settled=true in_flight=0 unsettled=[] wal_checkpoint=ok' in stderr_text, (
                f'[{label}] missing graceful shutdown log line; stderr was:\n{stderr_text}'
            )
            if expect_no_reclaim:
                assert 'queue startup: released' not in stderr_text, (
                    f'[{label}] startup found stale sending work when none should exist:\n{stderr_text}'
                )
            return stderr_text
        finally:
            if proc is not None and proc.poll() is None:
                os.kill(proc.pid, signal.SIGTERM)
                if not until(lambda: proc.poll() is not None, 3):
                    print(f'[{label}] process {proc.pid} ignored SIGTERM; no force kill attempted', file=sys.stderr)
                    raise RuntimeError('Probe process ignored SIGTERM')


def run_inflight_cycle(exe, env, token, data, log_path, front, label, delivery_id, proxy):
    """SIGTERM while the real dispatch/adapter path is inside an HTTP request."""
    proc = None
    with log_path.open('w+') as log:
        try:
            launched = time.monotonic()
            proc = sp.Popen(
                [exe, '--hidden'],
                env={
                    **os.environ,
                    **env,
                    'WIRE_STUDIO_KH_HIDDEN': '1',
                    'HTTPS_PROXY': f'http://127.0.0.1:{proxy.port}',
                    'https_proxy': f'http://127.0.0.1:{proxy.port}',
                },
                stdout=log,
                stderr=sp.STDOUT,
            )
            pid = proc.pid
            assert until(lambda: str(pid) in hold_owners() or proc.poll() is not None, 8), (
                f'[{label}] process {pid} did not bind hold port 8649'
            )
            assert proc.poll() is None, f'[{label}] process exited early rc={proc.returncode}'
            sending = until(
                lambda: row_tuple(data, delivery_id) and row_tuple(data, delivery_id)[0] == 'sending', 10
            )
            assert sending, f'[{label}] the queued delivery never entered sending (job not in flight)'
            assert proxy.seen, f'[{label}] the adapter never opened the proxy connection'
            before_state = row_tuple(data, delivery_id)
            print(
                f'[{label}] Job in flight at signal time: state={before_state[0]} attempts={before_state[1]} '
                f'proxy_connects={len(proxy.seen)}',
                flush=True,
            )

            hold = post(
                'http://127.0.0.1:8649/api/maintenance/hold?timeout_s=0&pause_s=25',
                {},
                'x-hold-token',
                token,
            )
            print(f'[{label}] Update hold while job is in flight: {json.dumps(hold, sort_keys=True)}', flush=True)
            assert hold.get('safe') is False, f'[{label}] hold should be unsafe while a job is in flight: {hold}'
            assert hold.get('in_flight') == 1, f'[{label}] hold did not observe in_flight=1: {hold}'
            assert hold.get('unsettled') == [delivery_id], f'[{label}] hold did not name the unsettled job: {hold}'

            time.sleep(max(0.0, 0.4 - (time.monotonic() - launched)))
            onscreen = visible_windows(pid)
            same_front = foreground() == front
            descendants = children(pid, processes())
            print(
                f'[{label}] Desktop safety before SIGTERM: PID={pid}; visible normal windows={onscreen}; '
                f'foreground unchanged={same_front}; descendants={descendants}',
                flush=True,
            )
            assert onscreen == 0 and same_front, (
                f'[{label}] Desktop safety failed: onscreen={onscreen}, same_front={same_front}'
            )

            begin = time.monotonic()
            os.kill(pid, signal.SIGTERM)
            watched = {pid, *descendants}
            gone = until(lambda: proc.poll() is not None and not (watched & processes().keys()), RACE_BOUND_S)
            elapsed = time.monotonic() - begin
            rc = proc.poll()
            survivors = sorted(watched & processes().keys())
            free = not hold_owners()
            same_front_after = foreground() == front
            after_state = row_tuple(data, delivery_id)
            no_hold_file = not any('hold' in entry.name for entry in data.iterdir() if entry.is_file())
            print(
                f'[{label}] SIGTERM: exit_code={rc}; all_gone={bool(gone)} in {elapsed:.3f}s; '
                f'survivors={survivors}; hold_port_free={free}; foreground_unchanged={same_front_after}; '
                f'no_persisted_hold_file={no_hold_file}; job_after={after_state}',
                flush=True,
            )

            log.flush()
            log.seek(0)
            stderr_text = log.read()
            assert gone and not survivors, f'[{label}] process or descendants survived SIGTERM: {survivors}'
            assert rc == 0, f'[{label}] expected graceful exit 0 on SIGTERM, observed rc={rc}'
            assert elapsed <= CONTRACT_MAX_S, f'[{label}] SIGTERM took {elapsed:.3f}s (> {CONTRACT_MAX_S}s contract)'
            assert elapsed >= 2.0, f'[{label}] shutdown returned in {elapsed:.3f}s without waiting for the in-flight job'
            assert free, f'[{label}] hold port 8649 still busy after exit'
            assert same_front_after, f'[{label}] foreground changed during SIGTERM'
            assert no_hold_file, f'[{label}] a hold file persisted in the data dir'
            assert after_state is not None, f'[{label}] delivery row disappeared during shutdown'
            assert after_state[0] != 'sending', f'[{label}] shutdown left the job half-written in sending: {after_state}'
            assert after_state[0] in ('pending', 'failed'), (
                f'[{label}] job ended in an unexpected state: {after_state}'
            )
            assert (after_state[3] or '').strip(), f'[{label}] job ended without a readable outcome: {after_state}'
            assert 'shutdown: signal=SIGTERM held=true settled=true in_flight=0 unsettled=[] wal_checkpoint=ok' in stderr_text, (
                f'[{label}] shutdown did not record a settled in-flight job; stderr was:\n{stderr_text}'
            )
            print(
                f'[{label}] Shutdown ended the in-flight job: state={after_state[0]} '
                f'outcome={after_state[3]!r}',
                flush=True,
            )
            return stderr_text
        finally:
            if proc is not None and proc.poll() is None:
                os.kill(proc.pid, signal.SIGTERM)
                if not until(lambda: proc.poll() is not None, 12):
                    raise RuntimeError('Probe process ignored SIGTERM')


def main():
    assert_free()
    root = Path(tempfile.mkdtemp(prefix='wire-sigterm-proof-')).resolve()
    keep = os.environ.get('KEEP_EVIDENCE') == '1'
    try:
        exe, desc = prepare_target(root)
        home, data = root / 'home', root / 'data'
        data.mkdir(parents=True)
        token_path = home / 'Library/Application Support/com.terranash.wirestudiokh/hold-token'
        token_path.parent.mkdir(parents=True)
        token = secrets.token_hex(24)
        token_path.touch(mode=0o600)
        token_path.write_text(token + '\n')
        env = {'HOME': str(home), 'TMPDIR': str(root), 'WIRE_STUDIO_KH_DATA_DIR': str(data)}

        # Phase 1: seed queued row via --test-mode and verify graceful SIGTERM there too.
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        seed_log = root / 'seed.log'
        with seed_log.open('w+') as log:
            seed = sp.Popen(
                [exe, '--test-mode', '--KH_TEST_PORT', str(port)],
                env={**os.environ, **env, 'WIRE_OPERATOR_TOKEN': token},
                stdout=log,
                stderr=sp.STDOUT,
            )
            try:
                assert until(lambda: (log.seek(0) or True) and 'TEST_DATA_DIR=' in log.read(), 6), (
                    'Headless seed fixture did not start'
                )
                queued = post(
                    f'http://127.0.0.1:{port}/api/test/trigger/schedule',
                    {'text': 'Isolated shutdown check', 'targets': ['telegram:TESTCHAT'], 'delay_s': 3600},
                    'x-wire-operator',
                    token,
                )
                assert queued.get('ok') and len(queued.get('delivery_ids', [])) == 1, f'Seed failed: {queued}'
                delivery_id = queued['delivery_ids'][0]
            finally:
                if seed.poll() is None:
                    seed.terminate()
                    seed.wait(timeout=10)
            log.flush()
            log.seek(0)
            seed_text = log.read()
        assert seed.returncode == 0, f'Seed --test-mode SIGTERM rc={seed.returncode}'
        assert 'shutdown: signal=SIGTERM' in seed_text and 'settled=true' in seed_text, (
            f'Seed --test-mode missing shutdown line:\n{seed_text}'
        )
        assert_free()
        print(f'PASS seed: {desc}; headless --test-mode SIGTERM exit=0; delivery={delivery_id}', flush=True)

        # Phase 2: real hidden desktop run_app() under active update hold + SIGTERM.
        front = foreground()
        run_hidden_cycle(exe, env, token, data, root / 'hidden-hold.log', front, 'held-sigterm')
        with sqlite3.connect((data / 'wire.db').as_uri() + '?mode=ro', uri=True) as db:
            integrity = db.execute('PRAGMA quick_check').fetchone()[0]
            row = db.execute(
                'SELECT state, attempts, claimed_at FROM deliveries WHERE id=?', (delivery_id,)
            ).fetchone()
        print(f'[held-sigterm] Queue read-only: quick_check={integrity}; row={row}', flush=True)
        assert integrity == 'ok' and row == ('pending', 0, None), f'Queue altered unexpectedly: {integrity} {row}'
        assert_free()

        # Phase 3: a real in-flight HTTP request through the real dispatch/adapter path, then SIGTERM.
        # The proxy withholds its answer; the app's shutdown path must wait for it and end the job.
        secret_path = data / 'app-secrets.json'
        secret_path.write_text(json.dumps({'telegram': {'bot_token': '123:FAKE'}}))
        secret_path.chmod(0o600)
        with sqlite3.connect(str(data / 'wire.db')) as db:
            db.execute(
                "UPDATE deliveries SET state='pending', attempts=0, claimed_at=NULL, "
                "last_error=NULL, next_attempt_at=? WHERE id=?",
                (now_z(), delivery_id),
            )
            db.commit()
        proxy = SlowProxy(4.0)
        proxy.start()
        try:
            inflight_stderr = run_inflight_cycle(
                exe,
                env,
                token,
                data,
                root / 'hidden-inflight.log',
                front,
                'inflight-sigterm',
                delivery_id,
                proxy,
            )
        finally:
            proxy.stop()
        assert 'queue startup: released' not in inflight_stderr, 'in-flight cycle unexpectedly reclaimed work at startup'
        with sqlite3.connect((data / 'wire.db').as_uri() + '?mode=ro', uri=True) as db:
            integrity = db.execute('PRAGMA quick_check').fetchone()[0]
        print(f'[inflight-sigterm] Queue read-only: quick_check={integrity}', flush=True)
        assert integrity == 'ok', f'Queue integrity after in-flight shutdown: {integrity}'
        assert_free()

        # Phase 4: restart after the settled in-flight job. Startup must find nothing stale.
        with sqlite3.connect(str(data / 'wire.db')) as db:
            db.execute(
                'UPDATE deliveries SET next_attempt_at=? WHERE id=?',
                ((datetime.now(timezone.utc) + timedelta(hours=1)).isoformat(timespec='seconds').replace('+00:00', 'Z'), delivery_id),
            )
            db.commit()
        clean_stderr = run_hidden_cycle(
            exe, env, token, data, root / 'hidden-clean-restart.log', front, 'clean-restart', expect_no_reclaim=True
        )
        assert 'queue startup: released' not in clean_stderr, 'clean restart found stale work'
        assert_free()

        # Phase 5: simulate an interrupted mid-send row left in SQLite, restart hidden desktop app,
        # verify startup reclaims it to pending so hold settles clean and SIGTERM exits 0.
        with sqlite3.connect(str(data / 'wire.db')) as db:
            db.execute(
                "UPDATE deliveries SET state='sending', attempts=1, claimed_at='2026-01-01T00:00:00+00:00' WHERE id=?",
                (delivery_id,),
            )
            db.commit()
        reclaim_stderr = run_hidden_cycle(
            exe, env, token, data, root / 'hidden-reclaim.log', front, 'startup-reclaim'
        )
        assert 'queue startup: released 1 interrupted sending delivery(ies) back to pending' in reclaim_stderr, (
            f'Missing startup reclaim log line:\n{reclaim_stderr}'
        )
        with sqlite3.connect((data / 'wire.db').as_uri() + '?mode=ro', uri=True) as db:
            integrity = db.execute('PRAGMA quick_check').fetchone()[0]
            row = db.execute(
                'SELECT state, attempts, claimed_at FROM deliveries WHERE id=?', (delivery_id,)
            ).fetchone()
        print(f'[startup-reclaim] Queue read-only: quick_check={integrity}; row={row}', flush=True)
        assert integrity == 'ok' and row == ('pending', 1, None), f'Stale sending not reclaimed: {integrity} {row}'
        assert_free()
        print(f'PASS all hidden SIGTERM checks ({desc})', flush=True)
    finally:
        if keep:
            print(f'Evidence kept at {root}', flush=True)
        else:
            shutil.rmtree(root, ignore_errors=True)


if __name__ == '__main__':
    main()
PY
