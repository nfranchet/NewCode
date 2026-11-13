#!/usr/bin/env python3
"""Test the workspace launcher and persistent headless sessions in a normal browser.

Build both binaries with cargo build. Only the test browser needs Chromium; editor
processes receive no desktop environment and no browser on PATH. Requires websocket-client.
"""
import argparse
import base64
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import select
import shutil
import signal
import subprocess
import tempfile
from urllib.request import Request, ProxyHandler, build_opener

spec = importlib.util.spec_from_file_location("server_smoke", Path(__file__).with_name("server-smoke.py"))
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)
wait_until = smoke.wait_until


def run(binary, browser):
    chromium = shutil.which(browser)
    assert chromium, "Install Chromium to run the test browser"
    with tempfile.TemporaryDirectory(prefix="focus-workspace-smoke-") as temporary:
        root = Path(temporary)
        processes, logs = [], []
        auth = "Basic " + base64.b64encode(b"smoke:fixture-password").decode()
        opener = build_opener(ProxyHandler({}))
        environment = dict(os.environ, XDG_DATA_HOME=str(root / "data"), XDG_CONFIG_HOME=str(root / "config"), SHELL="/bin/sh", PATH="/no-browser-installed")
        for name in ["DISPLAY", "WAYLAND_DISPLAY", "FOCUS_EDITOR_SERVER_USERNAME", "FOCUS_EDITOR_SERVER_PASSWORD"]:
            environment.pop(name, None)
        config = root / "server.toml"
        config.write_text("username='smoke'\npassword='fixture-password'\n")
        def launch(arguments, env):
            log = (root / f"process-{len(logs)}.log").open("w+")
            logs.append(log)
            proc = subprocess.Popen(arguments, env=env, stdout=subprocess.PIPE, stderr=log, text=True, start_new_session=True)
            processes.append(proc)
            return proc
        def server_start():
            proc = launch([str(binary.with_name("focus-editor-server")), "--bind", "127.0.0.1", "--port", "0", "--config", str(config)], environment)
            assert select.select([proc.stdout], [], [], 15)[0], "Server startup timed out"
            line = proc.stdout.readline().strip()
            assert line.startswith("Focus Editor server listening on http://127.0.0.1:"), line
            return proc, line.removeprefix("Focus Editor server listening on ")
        def catalog():
            return json.load(opener.open(Request(origin + "/api/editors", headers={"Authorization":auth}), timeout=5))
        a, b = root / "known workspace", root / "added workspace"
        for workspace in [a,b]:
            workspace.mkdir()
            (workspace / "example.txt").write_text("original\n")
        identity = hashlib.sha256(os.fsencode(a)).hexdigest()
        saved = root / "data/focus-editor/workspaces" / identity
        saved.mkdir(parents=True)
        (saved / "settings.toml").write_text("workspace=" + json.dumps(str(a)) + "\n")
        dev = None
        try:
            server, origin = server_start()
            profile = root / "browser"
            launch([chromium, f"--user-data-dir={profile}", "--headless=new", "--remote-debugging-port=0", "--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--no-proxy-server", "--no-first-run", "about:blank"], dict(os.environ))
            dev = smoke.debugger(profile)
            dev.send("Network.enable")
            dev.send("Network.setExtraHTTPHeaders", {"headers":{"Authorization":auth}})
            dev.send("Emulation.setDeviceMetricsOverride", {"width":1440,"height":1000,"deviceScaleFactor":1,"mobile":False})
            dev.send("Page.navigate", {"url":origin + "/"})
            wait_until(lambda:dev.evaluate(f"Boolean(document.querySelector('[data-workspace=\"{identity}\"] .start-workspace'))"))
            dev.evaluate(f"document.querySelector('[data-workspace=\"{identity}\"] .start-workspace').click()")
            first = wait_until(lambda: next((record for record in catalog() if record["workspace"] == str(a)), None))
            first_id = first["id"]
            def view(expression):
                return dev.evaluate(f"(() => {{ const w=document.querySelector('#view').contentWindow; return ({expression}); }})()")
            def ready():
                return view("w.smokeFrame?.meshes.length > 0 && w.document.querySelector('#notice').hidden")
            def claim():
                wait_until(ready)
                view("w.document.querySelector('#claim').click()")
                wait_until(lambda:view("w.smokeFrame?.control.owned"))
            def click(x,y):
                offset=dev.evaluate("(() => {const r=document.querySelector('#view').getBoundingClientRect(); return [r.x,r.y]})()")
                dev.click(x+offset[0],y+offset[1])
            def edit(text):
                click(150,127); click(600,102); dev.shortcut("a"); dev.text(text)
                wait_until(lambda:view("w.smokeFrame?.dirty"))
            claim(); edit("unsaved across server restart ✓")
            assert (a / "example.txt").read_text() == "original\n"
            print("Known workspace import, browser Start, and headless editing passed", flush=True)
            dev.evaluate("document.querySelector('#add-workspace').click()")
            dev.evaluate(f"document.querySelector('#workspace-path').value={json.dumps(str(b))}; document.querySelector('#add-form').requestSubmit()")
            b_identity = hashlib.sha256(os.fsencode(b)).hexdigest()
            wait_until(lambda:dev.evaluate(f"Boolean(document.querySelector('[data-workspace=\"{b_identity}\"] .start-workspace'))"))
            dev.evaluate(f"document.querySelector('[data-workspace=\"{b_identity}\"] .start-workspace').click()")
            second=wait_until(lambda:next((record for record in catalog() if record["workspace"] == str(b)),None))
            second_id=second["id"]
            wait_until(lambda:dev.evaluate(f"document.querySelector('#view').getAttribute('src') === '/editors/{second_id}/'"))
            claim(); edit("save from stop dialog 世界")
            dev.evaluate(f"document.querySelector('[data-id=\"{second_id}\"] .stop-editor').click()")
            wait_until(lambda:dev.evaluate("document.querySelector('#stop-dialog').open"))
            dev.evaluate("document.querySelector('#stop-cancel').click()")
            assert any(record["id"] == second_id for record in catalog())
            dev.evaluate(f"document.querySelector('[data-id=\"{second_id}\"] .stop-editor').click()")
            wait_until(lambda:dev.evaluate("document.querySelector('#stop-dialog').open"))
            dev.evaluate("document.querySelector('#stop-save').click()")
            wait_until(lambda:not any(record["id"] == second_id for record in catalog()))
            assert (b / "example.txt").read_text() == "save from stop dialog 世界"
            print("Add workspace, independent sessions, cancel, and save-and-stop passed", flush=True)
            # Send Ctrl+C to the gateway's process group; detached editors must survive.
            os.killpg(server.pid, signal.SIGINT)
            server.wait(timeout=10)
            assert os.kill(first["pid"] if "pid" in first else next(json.loads(path.read_text())["pid"] for path in (root / "data/focus-editor/runtime/instances").glob("*.json") if json.loads(path.read_text())["id"] == first_id), 0) is None
            server, origin = server_start()
            assert any(record["id"] == first_id for record in catalog())
            dev.send("Page.navigate", {"url":origin + "/#" + first_id})
            wait_until(ready)
            wait_until(lambda:view("w.smokeFrame?.dirty"))
            claim()
            assert (a / "example.txt").read_text() == "original\n"
            print("Server process-group shutdown and restart reattach the same unsaved editor", flush=True)
            screenshot=binary.parent / "workspace-smoke.png"
            screenshot.write_bytes(base64.b64decode(dev.send("Page.captureScreenshot", {"format":"png"})["data"]))
            dev.evaluate(f"document.querySelector('[data-id=\"{first_id}\"] .stop-editor').click()")
            wait_until(lambda:dev.evaluate("document.querySelector('#stop-dialog').open"))
            dev.evaluate("document.querySelector('#stop-discard').click()")
            wait_until(lambda:catalog() == [])
            assert (a / "example.txt").read_text() == "original\n"
            assert not dev.errors, dev.errors
            print("Discard-and-stop passed; screenshot:", screenshot, flush=True)
        except Exception:
            if dev:
                try:
                    (binary.parent / "workspace-smoke-failure.png").write_bytes(base64.b64decode(dev.send("Page.captureScreenshot", {"format":"png"})["data"]))
                    print("Browser state:",dev.evaluate("({error:document.querySelector('#action-error')?.textContent,status:document.querySelector('#status')?.textContent,stop:document.querySelector('#stop-error')?.textContent})"))
                except Exception: pass
            for index,log in enumerate(logs):
                log.seek(0); print(f"Process {index}:\n{log.read()[-5000:]}")
            raise
        finally:
            for path in (root / "data/focus-editor/runtime/instances").glob("*.json"):
                try:
                    record=json.loads(path.read_text())
                    opener.open(Request(record["endpoint"] + "/api/manage/stop",data=b'{"choice":"discard"}',headers={"Origin":record["endpoint"],"X-Focus-Token":record["token"]}), timeout=5).close()
                except (OSError,ValueError): pass
            if dev:
                try: dev.ws.close()
                except OSError: pass
            for proc in reversed(processes):
                if proc.poll() is None:
                    proc.terminate()
                    try: proc.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        os.killpg(proc.pid,signal.SIGKILL); proc.wait()
            for log in logs: log.close()


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary",type=Path,default=Path("target/debug/focus-editor"))
    parser.add_argument("--browser",default="chromium")
    args=parser.parse_args()
    run(args.binary.resolve(),args.browser)
