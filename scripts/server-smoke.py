#!/usr/bin/env python3
"""Exercise the authenticated hub in a normal browser over insecure HTTP.

Build with cargo build. Requires Chromium, an X11/Wayland desktop for local editors,
Python websocket-client, and a reachable local network address. All data is temporary.
"""
import argparse
import base64
import ctypes as C
import importlib.util
import json
import os
from pathlib import Path
import re
import select
import shlex
import shutil
import signal
import socket
import subprocess
import tempfile
import time
from urllib.request import Request, ProxyHandler, build_opener, urlopen

spec = importlib.util.spec_from_file_location("browser_smoke", Path(__file__).with_name("browser-smoke.py"))
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)
DevTools, wait_until = smoke.DevTools, smoke.wait_until

MONITOR = """
window.smokeFrame = null;
const originalFetch = window.fetch;
window.fetch = async (...args) => {
  if (String(args[0]).endsWith('/frame')) {
    const request = JSON.parse(args[1].body);
    window.smokeInput = request;
    if (request.events.length) { window.smokeEvents = request; (window.smokeHistory ||= []).push(request); window.smokeHistory = window.smokeHistory.slice(-15); }
  }
  const response = await originalFetch(...args);
  if (String(args[0]).endsWith('/frame') && response.status === 200) {
    response.clone().json().then(frame => { window.smokeFrame = frame; });
  }
  return response;
};
"""


def debugger(profile):
    wait_until(lambda: (profile / "DevToolsActivePort").exists())
    port = (profile / "DevToolsActivePort").read_text().splitlines()[0]
    tabs = json.load(urlopen(f"http://127.0.0.1:{port}/json/list"))
    dev = DevTools(next(tab["webSocketDebuggerUrl"] for tab in tabs if tab["type"] == "page"))
    dev.send("Page.enable")
    dev.send("Runtime.enable")
    dev.send("Page.addScriptToEvaluateOnNewDocument", {"source": MONITOR})
    return dev


def minimize_window(title):
    if not os.environ.get("DISPLAY"):
        return False
    x = C.CDLL("libX11.so.6")
    x.XOpenDisplay.argtypes = [C.c_char_p]
    x.XOpenDisplay.restype = C.c_void_p
    x.XDefaultRootWindow.argtypes = [C.c_void_p]
    x.XDefaultRootWindow.restype = C.c_ulong
    x.XQueryTree.argtypes = [C.c_void_p, C.c_ulong, C.POINTER(C.c_ulong), C.POINTER(C.c_ulong), C.POINTER(C.POINTER(C.c_ulong)), C.POINTER(C.c_uint)]
    x.XFetchName.argtypes = [C.c_void_p, C.c_ulong, C.POINTER(C.c_char_p)]
    x.XFree.argtypes = [C.c_void_p]
    x.XIconifyWindow.argtypes = [C.c_void_p, C.c_ulong, C.c_int]
    x.XFlush.argtypes = [C.c_void_p]
    x.XCloseDisplay.argtypes = [C.c_void_p]
    display = x.XOpenDisplay(None)
    if not display:
        return False
    def find(window):
        name = C.c_char_p()
        if x.XFetchName(display, window, C.byref(name)) and name.value:
            value = name.value.decode(errors="replace")
            x.XFree(name)
            if value == title:
                return window
        root, parent, count = C.c_ulong(), C.c_ulong(), C.c_uint()
        children = C.POINTER(C.c_ulong)()
        if x.XQueryTree(display, window, C.byref(root), C.byref(parent), C.byref(children), C.byref(count)):
            ids = list(children[:count.value])
            x.XFree(children)
            for child in ids:
                match = find(child)
                if match:
                    return match
        return None
    window = find(x.XDefaultRootWindow(display))
    if window:
        x.XIconifyWindow(display, window, 0)
        x.XFlush(display)
    x.XCloseDisplay(display)
    return bool(window)


def run(binary, browser):
    chromium = shutil.which(browser)
    assert chromium, "Install Chromium for the server smoke test"
    route = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    route.connect(("192.0.2.1", 80))  # Route lookup only: no packets are sent.
    address = route.getsockname()[0]
    route.close()
    assert not address.startswith("127."), "A local network address is needed for insecure-context coverage"
    with tempfile.TemporaryDirectory(prefix="focus-server-smoke-") as temporary:
        root = Path(temporary)
        processes, debuggers, logs = [], [], []
        auth = "Basic " + base64.b64encode(b"smoke:isolated-fixture-password").decode()
        opener = build_opener(ProxyHandler({}))
        environment = dict(os.environ, XDG_DATA_HOME=str(root / "data"), XDG_CONFIG_HOME=str(root / "config"), SHELL="/bin/sh")
        config = root / "server.toml"
        config.write_text("username = 'smoke'\npassword = 'isolated-fixture-password'\n")
        config.chmod(0o600)
        def launch(arguments, env=environment):
            log = (root / f"process-{len(logs)}.log").open("w+")
            logs.append(log)
            proc = subprocess.Popen(arguments, env=env, stdout=subprocess.PIPE, stderr=log, text=True, start_new_session=True)
            processes.append(proc)
            return proc
        def start_server():
            env = dict(environment, PATH="/no-browser-installed")
            env.pop("DISPLAY", None)
            env.pop("WAYLAND_DISPLAY", None)
            proc = launch([str(binary.with_name("focus-editor-server")), "--port", "0", "--config", str(config)], env)
            assert select.select([proc.stdout], [], [], 15)[0], "Server did not start"
            line = proc.stdout.readline().strip()
            match = re.search(r"http://0\.0\.0\.0:(\d+)$", line)
            assert match, line
            return proc, f"http://{address}:{match[1]}"
        def catalog():
            return json.load(opener.open(Request(origin + "/api/editors", headers={"Authorization": auth}), timeout=5))
        try:
            server, origin = start_server()
            assert catalog() == []
            normal_profile = root / "normal-browser"
            normal = launch([chromium, f"--user-data-dir={normal_profile}", "--headless=new", "--remote-debugging-port=0", "--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--no-proxy-server", "--no-first-run", "about:blank"])
            dev = debugger(normal_profile)
            debuggers.append(dev)
            dev.send("Network.enable")
            dev.send("Network.setExtraHTTPHeaders", {"headers": {"Authorization": auth}})
            dev.send("Emulation.setDeviceMetricsOverride", {"width": 1440, "height": 1000, "deviceScaleFactor": 1, "mobile": False})
            dev.send("Page.navigate", {"url": origin + "/"})
            wait_until(lambda: dev.evaluate("document.querySelector('#status')?.textContent === 'No editors are open'"))
            assert dev.evaluate("isSecureContext") is False
            assert dev.evaluate("typeof crypto.randomUUID") == "undefined"
            print("Headless server, public bind, Basic Auth, and ordinary HTTP browser passed", flush=True)

            # Start both local presentations after the gateway and browser already exist.
            bin_dir = root / "bin"
            bin_dir.mkdir()
            args_file = root / "app-browser-arguments"
            shim = bin_dir / "chromium"
            shim.write_text("#!/bin/sh\nprintf '%s\\n' \"$@\" > " + shlex.quote(str(args_file)) + "\nexec " + shlex.quote(chromium) + " --remote-debugging-port=0 --use-angle=swiftshader --enable-unsafe-swiftshader \"$@\"\n")
            shim.chmod(0o700)
            app_workspace = root / "app-workspace"
            native_workspace = root / ("native-workspace-" + root.name[-8:])
            for workspace in [app_workspace, native_workspace]:
                workspace.mkdir()
                (workspace / "example.txt").write_text("original\n")
            app = launch([str(binary), "--foreground", str(app_workspace)], dict(environment, PATH=str(bin_dir) + os.pathsep + os.environ["PATH"]))
            native = launch([str(binary), "--ui", "--foreground", str(native_workspace)])
            records = wait_until(lambda: (records if len(records := catalog()) == 2 else None))
            app_id = next(record["id"] for record in records if record["mode"] == "app")
            native_id = next(record["id"] for record in records if record["mode"] == "native")
            wait_until(args_file.exists)
            app_profile = Path(next(arg.split("=", 1)[1] for arg in args_file.read_text().splitlines() if arg.startswith("--user-data-dir=")))
            local = debugger(app_profile)
            debuggers.append(local)
            local.send("Emulation.setDeviceMetricsOverride", {"width": 1280, "height": 900, "deviceScaleFactor": 1, "mobile": False})
            local.send("Page.reload", {"ignoreCache": True})
            wait_until(lambda: local.evaluate("window.smokeFrame?.control.owned"))
            if os.environ.get("XDG_SESSION_TYPE") == "x11":
                wait_until(lambda: minimize_window("Focus Editor — " + native_workspace.name))
            local.send("Page.bringToFront")
            wait_until(lambda: local.evaluate("document.hasFocus()"))
            local.click(150, 127)
            local.click(600, 102)
            local.shortcut("a")
            local.text("Unsaved local buffer ✓")
            wait_until(lambda: local.evaluate("window.smokeFrame?.dirty"))
            assert (app_workspace / "example.txt").read_text() == "original\n"

            def view(expression):
                return dev.evaluate(f"(() => {{ const w = document.querySelector('#view').contentWindow; return ({expression}); }})()")
            def choose(identity):
                wait_until(lambda: dev.evaluate(f"Boolean(document.querySelector('[data-id=\"{identity}\"]'))"))
                dev.evaluate(f"document.querySelector('[data-id=\"{identity}\"] .editor-select').click()")
                wait_until(lambda: view("w.smokeFrame?.meshes.length > 0 && w.document.querySelector('#notice').hidden"))
            def click(x, y):
                offset = dev.evaluate("(() => {const r=document.querySelector('#view').getBoundingClientRect(); return [r.x,r.y]})()")
                dev.click(x + offset[0], y + offset[1])
            choose(app_id)
            wait_until(lambda: view("w.smokeFrame?.dirty"))
            assert view("w.smokeFrame.control.owned") is False
            view("w.document.querySelector('#claim').click()")
            wait_until(lambda: view("w.smokeFrame?.control.owned"))
            wait_until(lambda: local.evaluate("window.smokeFrame?.control.owned === false"))
            # Save the pre-existing unsaved buffer from the remote view.
            click(600, 102)
            dev.shortcut("s")
            wait_until(lambda: (app_workspace / "example.txt").read_text() == "Unsaved local buffer ✓")
            dev.shortcut("a")
            dev.text("Edited remotely over HTTP 世界")
            dev.shortcut("s")
            wait_until(lambda: (app_workspace / "example.txt").read_text() == "Edited remotely over HTTP 世界")
            print("Discovery of both modes, shared unsaved buffers, control handoff, and remote save passed", flush=True)

            # A second browser tab observes the same session and explicitly takes control.
            target = dev.send("Target.createTarget", {"url": "about:blank"})["targetId"]
            port = (normal_profile / "DevToolsActivePort").read_text().splitlines()[0]
            tabs = json.load(urlopen(f"http://127.0.0.1:{port}/json/list"))
            second = DevTools(next(tab["webSocketDebuggerUrl"] for tab in tabs if tab["id"] == target))
            debuggers.append(second)
            second.send("Page.enable")
            second.send("Runtime.enable")
            second.send("Page.addScriptToEvaluateOnNewDocument", {"source": MONITOR})
            second.send("Network.enable")
            second.send("Network.setExtraHTTPHeaders", {"headers": {"Authorization": auth}})
            second.send("Page.navigate", {"url": origin + "/#" + app_id})
            wait_until(lambda: second.evaluate("document.querySelector('#view')?.contentWindow?.smokeFrame?.control.owned === false"))
            second.evaluate("document.querySelector('#view').contentWindow.document.querySelector('#claim').click()")
            wait_until(lambda: second.evaluate("document.querySelector('#view').contentWindow.smokeFrame?.control.owned"))
            wait_until(lambda: view("w.smokeFrame?.control.owned === false"))
            second.send("Page.navigate", {"url": "about:blank"})
            assert app.poll() is None
            print("Multiple remote viewers, explicit controller transfer, and disconnect survival passed", flush=True)

            choose(native_id)
            minimized = minimize_window("Focus Editor — " + native_workspace.name)
            view("w.document.querySelector('#claim').click()")
            wait_until(lambda: view("w.smokeFrame?.control.owned"))
            click(150, 127)
            click(600, 102)
            dev.shortcut("a")
            dev.text("Native session edited remotely ✓")
            dev.shortcut("s")
            wait_until(lambda: (native_workspace / "example.txt").read_text() == "Native session edited remotely ✓")
            click(600, 850)
            marker = root / "remote-terminal"
            dev.text(f"printf REMOTE_TERMINAL > {marker}")
            dev.key("Enter", "Enter")
            wait_until(marker.exists)
            assert marker.read_text() == "REMOTE_TERMINAL"
            print("Native shared renderer, remote editing, and live PTY passed (minimized: " + str(minimized) + ")", flush=True)
            destination = root / "remotely-picked-workspace"
            destination.mkdir()
            click(60, 20)
            time.sleep(0.3)
            dev.shortcut("a")
            dev.text(str(destination))
            dev.key("Enter", "Enter", 2)
            wait_until(lambda: any(record["id"] == native_id and record["workspace"] == str(destination) for record in catalog()))
            wait_until(lambda: view("w.smokeFrame?.title === 'Focus Editor — remotely-picked-workspace'"))
            print("Remote host filesystem picker and workspace discovery update passed", flush=True)

            # Return to the app, leave a dirty buffer, and stop the gateway.
            choose(app_id)
            view("w.document.querySelector('#claim').click()")
            wait_until(lambda: view("w.smokeFrame?.control.owned"))
            click(600, 102)
            dev.key("End", "End", 2)
            dev.text(" gateway survives")
            wait_until(lambda: view("w.smokeFrame?.dirty"))
            screenshot = binary.parent / "server-smoke.png"
            screenshot.write_bytes(base64.b64decode(dev.send("Page.captureScreenshot", {"format": "png"})["data"]))
            server.terminate()
            server.wait(timeout=10)
            assert app.poll() is None and native.poll() is None
            local.evaluate("document.querySelector('#claim').click()")
            wait_until(lambda: local.evaluate("window.smokeFrame?.control.owned"))
            local.click(600, 102)
            local.shortcut("s")
            wait_until(lambda: (app_workspace / "example.txt").read_text() == "Edited remotely over HTTP 世界 gateway survives")
            server, origin = start_server()
            assert {record["id"] for record in catalog()} == {app_id, native_id}
            print("Gateway shutdown/restart preserves both local editors and unsaved state", flush=True)
            assert not dev.errors and not local.errors and not second.errors, [dev.errors, local.errors, second.errors]
            print("Server browser smoke passed; screenshot:", screenshot, flush=True)
        except Exception:
            if 'dev' in locals():
                try:
                    print('Remote state:', dev.evaluate("(() => { const w = document.querySelector('#view').contentWindow; const f = w.smokeFrame; return {frame: f && {control:f.control,size:f.size,dirty:f.dirty}, input:w.smokeInput,events:w.smokeHistory, focus:w.document.activeElement.id, secure:w.isSecureContext, notice:w.document.querySelector('#message').textContent}; })()"))
                    (binary.parent / 'server-smoke-failure.png').write_bytes(base64.b64decode(dev.send('Page.captureScreenshot', {'format':'png'})['data']))
                    print('Fixture contents:', (app_workspace / 'example.txt').read_text(), (native_workspace / 'example.txt').read_text())
                except Exception as diagnostic: print('Diagnostic failed:', diagnostic)
            for index, log in enumerate(logs):
                log.seek(0)
                # Do not print private app URLs/tokens from stdout.
                print(f"Process {index} diagnostics:\n{log.read()[-6000:]}")
            raise
        finally:
            # Stop editors through their private API so owned browsers/PTYs are reaped.
            directory = root / "data/focus-editor/runtime/instances"
            for registration in directory.glob("*.json"):
                try:
                    record = json.loads(registration.read_text())
                    headers = {"Origin": record["endpoint"], "X-Focus-Token": record["token"], "X-Focus-Client": "smoke-cleanup"}
                    for action in ["connect", "shutdown"]:
                        opener.open(Request(record["endpoint"] + "/api/" + action, data=b"", headers=headers), timeout=5).close()
                except (OSError, ValueError):
                    pass
            for dev in debuggers:
                try: dev.ws.close()
                except OSError: pass
            for proc in reversed(processes):
                if proc.poll() is None:
                    proc.terminate()
                    try: proc.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        os.killpg(proc.pid, signal.SIGKILL)
                        proc.wait()
            for log in logs: log.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/focus-editor"))
    parser.add_argument("--browser", default="chromium")
    args = parser.parse_args()
    run(args.binary.resolve(), args.browser)
