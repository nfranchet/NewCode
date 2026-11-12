#!/usr/bin/env python3
"""Exercise the sandboxed Chromium app window. Requires a desktop, websocket-client, and Pillow.

Build first with cargo build, then run python3 scripts/browser-smoke.py.
All application data, browser profiles, and edited files are isolated in a temporary folder.
"""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import select
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import tomllib
from urllib.request import urlopen

import websocket


def wait_until(predicate, timeout=15):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        time.sleep(0.05)
    raise AssertionError("Timed out waiting for the browser smoke test")


class DevTools:
    def __init__(self, url):
        self.ws = websocket.create_connection(url, timeout=15, suppress_origin=True)
        self.sequence = 0
        self.errors = []
        self.dialogs = 0
        self.accept_dialogs = True

    def event(self, message):
        if message.get("method") == "Runtime.exceptionThrown":
            self.errors.append(message["params"]["exceptionDetails"].get("text", "JavaScript exception"))
        if message.get("method") == "Page.javascriptDialogOpening":
            self.dialogs += 1
            self.sequence += 1
            self.ws.send(json.dumps({"id": self.sequence, "method": "Page.handleJavaScriptDialog", "params": {"accept": self.accept_dialogs}}))

    def send(self, method, params=None):
        self.sequence += 1
        request_id = self.sequence
        self.ws.send(json.dumps({"id": request_id, "method": method, "params": params or {}}))
        while True:
            message = json.loads(self.ws.recv())
            self.event(message)
            if message.get("id") == request_id:
                if "error" in message:
                    raise AssertionError(message["error"])
                return message.get("result", {})

    def evaluate(self, expression):
        result = self.send("Runtime.evaluate", {"expression": expression, "returnByValue": True, "awaitPromise": True, "userGesture": True})
        if "exceptionDetails" in result:
            raise AssertionError(result["exceptionDetails"].get("text", "JavaScript evaluation failed"))
        return result["result"].get("value")

    def pump(self, seconds=0.3):
        deadline = time.monotonic() + seconds
        self.ws.settimeout(0.1)
        try:
            while time.monotonic() < deadline:
                try:
                    message = self.ws.recv()
                    if not message:
                        break
                    self.event(json.loads(message))
                except websocket.WebSocketTimeoutException:
                    continue
                except websocket.WebSocketConnectionClosedException:
                    break
        finally:
            self.ws.settimeout(15)

    def click(self, x, y):
        self.send("Input.dispatchMouseEvent", {"type": "mouseMoved", "x": x, "y": y})
        self.send("Input.dispatchMouseEvent", {"type": "mousePressed", "x": x, "y": y, "button": "left", "clickCount": 1})
        self.send("Input.dispatchMouseEvent", {"type": "mouseReleased", "x": x, "y": y, "button": "left", "clickCount": 1})
        time.sleep(0.12)

    def key(self, key, code, modifiers=0):
        virtual = ord(key.upper()) if len(key) == 1 else {"Enter": 13, "End": 35, "Home": 36, "Escape": 27}.get(key, 0)
        self.send("Input.dispatchKeyEvent", {"type": "rawKeyDown", "key": key, "code": code, "modifiers": modifiers, "windowsVirtualKeyCode": virtual})
        self.send("Input.dispatchKeyEvent", {"type": "keyUp", "key": key, "code": code, "modifiers": modifiers, "windowsVirtualKeyCode": virtual})
        time.sleep(0.1)

    def shortcut(self, key):
        self.key(key, "Key" + key.upper(), 2)

    def text(self, text):
        self.send("Input.insertText", {"text": text})
        time.sleep(0.15)


def run(binary, browser, screenshot, addons):
    with tempfile.TemporaryDirectory(prefix="focus-browser-smoke-") as temporary:
        root = Path(temporary)
        workspace = root / "workspace"
        workspace.mkdir()
        source = workspace / "example.txt"
        source.write_text("original\n")
        addon_ids = []
        for package in addons:
            manifest_path = package / "addon.toml"
            manifest = tomllib.loads(manifest_path.read_text())
            destination = root / "data/focus-editor/addons" / manifest["id"]
            destination.mkdir(parents=True)
            shutil.copy2(manifest_path, destination / "addon.toml")
            shutil.copy2(binary.parent / manifest["executable"], destination / manifest["executable"])
            if manifest.get("icon"):
                shutil.copy2(package / manifest["icon"], destination / manifest["icon"])
            addon_ids.append(manifest["id"])
        if addon_ids:
            identity = hashlib.sha256(os.fsencode(workspace.resolve())).hexdigest()
            settings = root / "data/focus-editor/workspaces" / identity / "settings.toml"
            settings.parent.mkdir(parents=True)
            settings.write_text("[addon_enabled]\n" + "".join(f"{json.dumps(id)} = true\n" for id in addon_ids)
                + "".join(f"\n[addon_state.{json.dumps(id)}]\nautostart = false\n" for id in addon_ids))
        real_browser = shutil.which(browser)
        assert real_browser, "Install Chromium/Chrome before running the app-window smoke test"
        shims = root / "bin"
        shims.mkdir()
        shim = shims / "chromium"
        arguments_file = root / "browser-arguments"
        # Attach the test debugger to the product's actual app window. Keep its sandbox enabled.
        shim.write_text("#!/bin/sh\numask 077\nprintf '%s\\n' \"$@\" > \"$FOCUS_BROWSER_ARGS\"\nprintf '%s' \"$$\" > \"$FOCUS_BROWSER_ARGS.pid\"\nexec "
            + shlex.quote(real_browser) + " --remote-debugging-port=0 --use-angle=swiftshader --enable-unsafe-swiftshader \"$@\"\n")
        shim.chmod(0o700)
        external_opener = root / "external-opener"
        external_opener.write_text('#!/bin/sh\nprintf "%s" "$1" > "$FOCUS_EXTERNAL_LINK"\n')
        external_opener.chmod(0o700)
        environment = dict(os.environ, XDG_DATA_HOME=str(root / "data"), XDG_CONFIG_HOME=str(root / "config"), SHELL="/bin/sh",
            PATH=str(shims) + os.pathsep + os.environ.get("PATH", ""), BROWSER=str(external_opener), RUST_BACKTRACE="1",
            FOCUS_BROWSER_ARGS=str(arguments_file), FOCUS_EXTERNAL_LINK=str(root / "external-link"))
        app_log = (root / "app.log").open("w+")
        app = subprocess.Popen([str(binary), "--foreground", str(workspace)], env=environment, stdout=subprocess.PIPE, stderr=app_log, text=True)
        try:
            ready = select.select([app.stdout], [], [], 20)[0]
            assert ready, "Editor did not print its browser URL"
            line = app.stdout.readline().strip()
            assert line.startswith("Focus Editor: http://127.0.0.1:"), "Editor did not select browser mode"
            url = line.removeprefix("Focus Editor: ")
            origin = url.split("/#")[0]
            wait_until(arguments_file.exists)
            arguments = arguments_file.read_text().splitlines()
            assert f"--app={url}" in arguments
            assert "--no-sandbox" not in arguments and "--disable-web-security" not in arguments
            profile = Path(next(arg.removeprefix("--user-data-dir=") for arg in arguments if arg.startswith("--user-data-dir=")))
            assert profile.stat().st_mode & 0o777 == 0o700
            wait_until(lambda: (profile / "DevToolsActivePort").exists())
            port = (profile / "DevToolsActivePort").read_text().splitlines()[0]
            with urlopen(f"http://127.0.0.1:{port}/json/list") as response:
                tabs = json.load(response)
            dev = DevTools(next(tab["webSocketDebuggerUrl"] for tab in tabs if tab["type"] == "page"))
            dev.send("Page.enable")
            dev.send("Runtime.enable")
            wait_until(lambda: dev.evaluate("document.title === 'Focus Editor — workspace'"))
            assert dev.evaluate("document.querySelectorAll('header').length") == 0
            assert dev.evaluate("window.outerHeight - window.innerHeight < 90"), "Browser navigation controls are visible"
            if sys.platform.startswith("linux"):
                browser_pid = int(Path(str(arguments_file) + ".pid").read_text())
                def renderer_is_sandboxed():
                    for process in Path("/proc").iterdir():
                        if not process.name.isdigit():
                            continue
                        try:
                            command = (process / "cmdline").read_bytes()
                            if b"--type=renderer" not in command or os.getpgid(int(process.name)) != browser_pid:
                                continue
                            status = dict(line.split(":", 1) for line in (process / "status").read_text().splitlines())
                            if status.get("NoNewPrivs", "").strip() == "1" and status.get("Seccomp", "").strip() == "2":
                                return True
                        except (OSError, ProcessLookupError):
                            continue
                    return False
                wait_until(renderer_is_sandboxed)
                print("Dedicated app window, private profile, and renderer sandbox passed", flush=True)
            dev.send("Emulation.setDeviceMetricsOverride", {"width": 1280, "height": 900, "deviceScaleFactor": 1, "mobile": False})
            dev.send("Page.addScriptToEvaluateOnNewDocument", {"source": """
                window.smokeFrame = null;
                window.smokeDocument = crypto.randomUUID();
                const originalFetch = window.fetch;
                window.fetch = async (...args) => {
                    window.smokeHeaders = args[1]?.headers;
                    const response = await originalFetch(...args);
                    if (String(args[0]).endsWith('/api/frame') && response.status === 200) {
                        response.clone().json().then(frame => { window.smokeFrame = {
                            dirty: frame.dirty, title: frame.title,
                            meshes: frame.meshes.length, scale: frame.pixels_per_point
                        }; });
                    }
                    return response;
                };
            """})
            dev.send("Page.reload", {"ignoreCache": True})
            wait_until(lambda: dev.evaluate("window.smokeFrame?.meshes > 0"))
            assert dev.evaluate("document.title") == "Focus Editor — workspace"
            assert dev.evaluate("document.querySelector('#notice').hidden")
            print("Browser launch and WebGL rendering passed", flush=True)
            if addon_ids:
                wait_until(lambda: all(f"Addon {id}: stop requested" in (root / "app.log").read_text() for id in addon_ids))
                if "opencode" in addon_ids and "codex" in addon_ids:
                    dev.click(1140, 62)
                print("Offline addon surfaces started:", ", ".join(addon_ids), flush=True)

            dev.evaluate("fetch('/api/open-url', {method: 'POST', headers: window.smokeHeaders, body: JSON.stringify({url: 'https://example.invalid/editor-link'})}).then(response => response.ok)")
            wait_until(lambda: (root / "external-link").exists())
            assert (root / "external-link").read_text() == "https://example.invalid/editor-link"
            assert dev.evaluate("location.origin") == origin
            print("External links use the system browser", flush=True)

            dev.click(150, 127)  # First Explorer entry, below the project header.
            dev.click(600, 102)
            dev.shortcut("a")
            expected = "Browser edit: café 世界\n"
            dev.text(expected)
            wait_until(lambda: dev.evaluate("window.smokeFrame?.dirty"))
            dev.shortcut("s")
            wait_until(lambda: source.read_text() == expected)
            wait_until(lambda: dev.evaluate("window.smokeFrame?.dirty === false"))
            print("Unicode input and save passed", flush=True)

            dev.send("Browser.grantPermissions", {"origin": origin, "permissions": ["clipboardReadWrite", "clipboardSanitizedWrite"]})
            dev.shortcut("a")
            dev.shortcut("c")
            wait_until(lambda: dev.evaluate("navigator.clipboard.readText()") == expected)
            dev.evaluate("navigator.clipboard.writeText('Clipboard paste ✓')")
            dev.shortcut("v")
            dev.shortcut("s")
            expected = "Clipboard paste ✓"
            wait_until(lambda: source.read_text() == expected)
            print("Clipboard copy and paste passed", flush=True)

            dev.key("End", "End", 2)
            dev.text(" unsaved reload")
            wait_until(lambda: dev.evaluate("window.smokeFrame?.dirty"))
            previous_document = dev.evaluate("window.smokeDocument")
            dev.send("Page.reload", {"ignoreCache": True})
            wait_until(lambda: dev.evaluate(f"window.smokeDocument !== {json.dumps(previous_document)} && window.smokeFrame?.dirty && document.querySelector('#notice').hidden"))
            dev.click(600, 102)
            dev.shortcut("s")
            expected += " unsaved reload"
            wait_until(lambda: source.read_text() == expected)
            assert dev.dialogs >= 1, "Missing unsaved-change unload warning"
            print("Unsaved buffer and title survive reload", flush=True)

            dev.click(600, 730)
            marker = root / "terminal-marker"
            pid_file = root / "terminal.pid"
            dev.text(f"printf TERMINAL_OK > {marker}; printf $$ > {pid_file}")
            dev.key("Enter", "Enter")
            wait_until(marker.exists)
            assert marker.read_text() == "TERMINAL_OK"
            shell_pid = int(pid_file.read_text())
            dev.text("sleep 30")
            dev.key("Enter", "Enter")
            dev.shortcut("c")
            interrupted = root / "interrupted"
            dev.text(f"printf INTERRUPTED > {interrupted}")
            dev.key("Enter", "Enter")
            wait_until(interrupted.exists)
            print("Terminal commands and Ctrl+C passed", flush=True)

            dev.send("Emulation.setDeviceMetricsOverride", {"width": 1000, "height": 720, "deviceScaleFactor": 2, "mobile": False})
            wait_until(lambda: dev.evaluate("window.smokeFrame?.scale === 2"))
            # Addon surfaces render asynchronously; let them receive the new panel bounds.
            time.sleep(0.75)
            assert dev.evaluate("document.querySelector('#editor').getContext('webgl2').getError()") == 0
            screenshot.parent.mkdir(parents=True, exist_ok=True)
            screenshot.write_bytes(base64.b64decode(dev.send("Page.captureScreenshot", {"format": "png"})["data"]))
            from PIL import Image
            with Image.open(screenshot) as image:
                assert len(image.convert("RGB").getcolors(image.width * image.height)) > 100, "Canvas did not render text and controls"
            print("Resize and high DPI rendering passed", flush=True)

            # Ensure shutdown confirmation covers unsaved edits too.
            dev.click(600, 102)
            dev.text("unsaved on exit")
            wait_until(lambda: dev.evaluate("window.smokeFrame?.dirty"))
            dialogs = dev.dialogs
            dev.accept_dialogs = False
            dev.send("Page.close")
            dev.pump(0.5)
            assert dev.dialogs > dialogs and app.poll() is None, "Cancelling close lost the editor session"
            dev.accept_dialogs = True
            dialogs = dev.dialogs
            dev.send("Page.close")
            dev.pump(0.5)
            wait_until(lambda: app.poll() is not None)
            assert app.returncode == 0
            assert dev.dialogs > dialogs
            wait_until(lambda: not Path(f"/proc/{shell_pid}").exists())
            wait_until(lambda: not profile.exists())
            assert not dev.errors, dev.errors
            app_log.flush()
            assert url not in (root / "app.log").read_text(), "Session credential leaked to diagnostic output"
            print(f"Window close confirmation, PTY cleanup, and profile cleanup passed; screenshot: {screenshot}", flush=True)
            dev.ws.close()
        except Exception:
            if "dev" in locals():
                try:
                    failure = screenshot.with_name(screenshot.stem + "-failure.png")
                    failure.parent.mkdir(parents=True, exist_ok=True)
                    failure.write_bytes(base64.b64decode(dev.send("Page.captureScreenshot", {"format": "png"})["data"]))
                    print("Browser failure screenshot:", failure)
                    print("Browser state:", dev.evaluate("({frame: window.smokeFrame, focus: document.activeElement.id, status: document.querySelector('#message')?.textContent})"))
                    print("Fixture document:", repr(source.read_text()))
                except Exception:
                    pass
            # Print diagnostics without exposing the session URL or its token.
            app_log.seek(0)
            diagnostic = app_log.read()
            if "url" in locals():
                diagnostic = diagnostic.replace(url, "[private session URL]")
            print(diagnostic)
            raise
        finally:
            if app.poll() is None:
                # Exercise the host's normal shutdown path so it can reap its browser too.
                if "dev" in locals():
                    try:
                        dev.evaluate("fetch('/api/shutdown', {method: 'POST', headers: window.smokeHeaders})")
                        app.wait(timeout=5)
                    except Exception:
                        pass
                if app.poll() is None:
                    try:
                        owned_pid = int(Path(str(arguments_file) + ".pid").read_text())
                        if os.getpgid(owned_pid) == owned_pid:
                            os.killpg(owned_pid, signal.SIGTERM)
                            app.wait(timeout=5)
                    except (OSError, ValueError, subprocess.TimeoutExpired):
                        pass
                if app.poll() is None:
                    app.terminate()
                    try:
                        app.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        app.kill()
                        app.wait()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/focus-editor"))
    parser.add_argument("--browser", default="chromium")
    parser.add_argument("--screenshot", type=Path, default=Path("target/browser-smoke.png"))
    parser.add_argument("--addon", type=Path, action="append", default=[], help="Also install a built addon package for offline surface coverage")
    args = parser.parse_args()
    run(args.binary.resolve(), args.browser, args.screenshot, args.addon)
