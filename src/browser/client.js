// Browser integration for the shared Rust/egui application. No remote assets or Node runtime.
const canvas = document.querySelector("#editor");
const input = document.querySelector("#input");
const notice = document.querySelector("#notice");
const message = document.querySelector("#message");
const reconnect = document.querySelector("#reconnect");
const token = location.hash.slice(1);
const remoteId = location.pathname.match(/^\/editors\/([a-f0-9]{48})\/$/)?.[1];
const remote = Boolean(remoteId);
const apiBase = remote ? `/api/editors/${remoteId}/` : "/api/";
// getRandomValues works on ordinary HTTP as well as secure browser contexts.
const client = Array.from(crypto.getRandomValues(new Uint8Array(24)), byte => byte.toString(16).padStart(2, "0")).join("");
const controlBar = document.querySelector("#control");
const ownerLabel = document.querySelector("#owner");
const claimButton = document.querySelector("#claim");
const externalLink = document.querySelector("#external-link");
let control = { owned: false, epoch: 0 };
let view = { scale: 1, x: 0, y: 0, zoom: 1 };
function setControl(next) {
  if (next.epoch < control.epoch) return;
  if (next.epoch !== control.epoch || next.owned !== control.owned) {
    events = [];
    composing = false;
    input.value = "";
  }
  control = next;
  controlBar.hidden = control.owned;
  ownerLabel.textContent = control.owner ? `${control.owner} has control.` : "Viewing this editor.";
}
const isMac = /Mac|iPhone|iPad/.test(navigator.platform);
let events = [], running = false, dirty = false, composing = false, timer, inFlight;
let lastComposition = null;


function showNotice(text, retry = false) {
  externalLink.hidden = true;
  message.textContent = text;
  notice.hidden = false;
  reconnect.hidden = !retry;
}

async function api(path, body = "", keepalive = false) {
  const response = await fetch(`${apiBase}${path}`, {
    method: "POST", body, keepalive, cache: "no-store",
    headers: { "X-Focus-Token": token, "X-Focus-Client": client, "Content-Type": "application/json" },
  });
  if (!response.ok) throw new Error(await response.text());
  return response;
}

function bytes(base64) {
  const raw = atob(base64);
  return Uint8Array.from(raw, c => c.charCodeAt(0));
}

class Painter {
  constructor(canvas) {
    const gl = canvas.getContext("webgl2", { alpha: false, antialias: false, depth: false, stencil: false });
    if (!gl) throw new Error("WebGL 2 is required. Enable browser graphics acceleration or launch focus-editor --ui.");
    this.gl = gl;
    this.textures = new Map();
    const shader = (type, source) => {
      const result = gl.createShader(type);
      gl.shaderSource(result, source);
      gl.compileShader(result);
      if (!gl.getShaderParameter(result, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(result));
      return result;
    };
    const vertex = shader(gl.VERTEX_SHADER, `#version 300 es
      precision highp float;
      uniform vec2 screen;
      in vec2 position;
      in vec2 uv;
      in vec4 color;
      out vec2 texcoord;
      out vec4 rgba;
      void main() {
        gl_Position = vec4(position.x * 2.0 / screen.x - 1.0, 1.0 - position.y * 2.0 / screen.y, 0, 1);
        texcoord = uv;
        rgba = color;
      }`);
    const fragment = shader(gl.FRAGMENT_SHADER, `#version 300 es
      precision highp float;
      uniform sampler2D image;
      in vec2 texcoord;
      in vec4 rgba;
      out vec4 pixel;
      void main() { pixel = rgba * texture(image, texcoord); }`);
    this.program = gl.createProgram();
    gl.attachShader(this.program, vertex);
    gl.attachShader(this.program, fragment);
    gl.linkProgram(this.program);
    if (!gl.getProgramParameter(this.program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(this.program));
    gl.deleteShader(vertex);
    gl.deleteShader(fragment);
    gl.useProgram(this.program);
    this.screen = gl.getUniformLocation(this.program, "screen");
    gl.uniform1i(gl.getUniformLocation(this.program, "image"), 0);
    this.vertexBuffer = gl.createBuffer();
    this.indexBuffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, this.vertexBuffer);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, this.indexBuffer);
    for (const [name, count, type, normalized, offset] of [
      ["position", 2, gl.FLOAT, false, 0], ["uv", 2, gl.FLOAT, false, 8], ["color", 4, gl.UNSIGNED_BYTE, true, 16],
    ]) {
      const location = gl.getAttribLocation(this.program, name);
      gl.enableVertexAttribArray(location);
      gl.vertexAttribPointer(location, count, type, normalized, 20, offset);
    }
    gl.enable(gl.BLEND);
    // egui colors and textures are premultiplied sRGBA, matching its native glow renderer.
    gl.blendFuncSeparate(gl.ONE, gl.ONE_MINUS_SRC_ALPHA, gl.ONE_MINUS_DST_ALPHA, gl.ONE);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
  }

  reset() {
    for (const texture of this.textures.values()) this.gl.deleteTexture(texture);
    this.textures.clear();
  }

  paint(frame) {
    const gl = this.gl;
    if (frame.reset) this.reset();
    for (const update of frame.textures) {
      let texture = this.textures.get(update.id);
      if (!texture) { texture = gl.createTexture(); this.textures.set(update.id, texture); }
      gl.bindTexture(gl.TEXTURE_2D, texture);
      const pixels = bytes(update.pixels);
      if (update.pos) {
        gl.texSubImage2D(gl.TEXTURE_2D, 0, ...update.pos, ...update.size, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
      } else {
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, ...update.size, 0, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
      }
      const options = update.options;
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, options.minification === "Nearest" ? gl.NEAREST : gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, options.magnification === "Nearest" ? gl.NEAREST : gl.LINEAR);
      const wrap = { ClampToEdge: gl.CLAMP_TO_EDGE, Repeat: gl.REPEAT, MirroredRepeat: gl.MIRRORED_REPEAT }[options.wrap_mode];
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, wrap);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, wrap);
    }
    const dpr = Math.min(4, Math.max(0.5, devicePixelRatio || 1));
    const width = Math.round(canvas.clientWidth * dpr), height = Math.round(canvas.clientHeight * dpr);
    if (canvas.width !== width || canvas.height !== height) { canvas.width = width; canvas.height = height; }
    const displayScale = Math.min(canvas.clientWidth / frame.size[0], canvas.clientHeight / frame.size[1]);
    view = { scale: displayScale,
      x: (canvas.clientWidth - frame.size[0] * displayScale) / 2,
      y: (canvas.clientHeight - frame.size[1] * displayScale) / 2,
      zoom: frame.pixels_per_point / dpr };
    const scale = displayScale * dpr;
    const viewportWidth = Math.round(frame.size[0] * scale), viewportHeight = Math.round(frame.size[1] * scale);
    const offsetX = Math.round(view.x * dpr), offsetY = Math.round(view.y * dpr);
    gl.viewport(offsetX, height - offsetY - viewportHeight, viewportWidth, viewportHeight);
    gl.disable(gl.SCISSOR_TEST);
    gl.clearColor(0.93, 0.94, 0.96, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.uniform2f(this.screen, ...frame.size);
    gl.enable(gl.SCISSOR_TEST);
    for (const mesh of frame.meshes) {
      const texture = this.textures.get(mesh.texture);
      if (!texture) continue;
      const [left, top, right, bottom] = mesh.clip;
      const x = Math.max(0, offsetX + Math.round(left * scale)), y = Math.max(0, offsetY + Math.round(top * scale));
      const x2 = Math.min(width, offsetX + Math.round(right * scale)), y2 = Math.min(height, offsetY + Math.round(bottom * scale));
      if (x2 <= x || y2 <= y) continue;
      gl.scissor(x, height - y2, x2 - x, y2 - y);
      gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.bufferData(gl.ARRAY_BUFFER, bytes(mesh.vertices), gl.STREAM_DRAW);
      const indices = bytes(mesh.indices);
      gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, indices, gl.STREAM_DRAW);
      gl.drawElements(gl.TRIANGLES, indices.byteLength / 4, gl.UNSIGNED_INT, 0);
    }
    for (const id of frame.free) { gl.deleteTexture(this.textures.get(id)); this.textures.delete(id); }
  }
}

const cursors = {
  Default: "default", None: "none", PointingHand: "pointer", Text: "text", VerticalText: "vertical-text",
  Grab: "grab", Grabbing: "grabbing", Crosshair: "crosshair", Move: "move", NotAllowed: "not-allowed",
  NoDrop: "no-drop", Wait: "wait", Progress: "progress", Help: "help", ContextMenu: "context-menu",
  Alias: "alias", Copy: "copy", AllScroll: "all-scroll", ZoomIn: "zoom-in", ZoomOut: "zoom-out",
  ResizeHorizontal: "ew-resize", ResizeVertical: "ns-resize", ResizeNeSw: "nesw-resize", ResizeNwSe: "nwse-resize",
  ResizeEast: "e-resize", ResizeWest: "w-resize", ResizeNorth: "n-resize", ResizeSouth: "s-resize",
  ResizeNorthEast: "ne-resize", ResizeNorthWest: "nw-resize", ResizeSouthEast: "se-resize", ResizeSouthWest: "sw-resize",
  ResizeColumn: "col-resize", ResizeRow: "row-resize",
};

async function copyText(text) {
  try { await navigator.clipboard.writeText(text); }
  catch {
    input.value = text;
    input.focus({ preventScroll: true });
    input.select();
    if (!document.execCommand("copy")) showNotice("Clipboard permission was denied. Allow clipboard access in your browser.");
    input.value = "";
  }
}

async function pasteText() {
  try { queue({ Paste: await navigator.clipboard.readText() }); }
  catch { showNotice("Use Ctrl+V (Command+V on macOS) to paste, or allow clipboard access in your browser."); }
}

function platform(frame) {
  dirty = frame.dirty;
  if (frame.title) document.title = frame.title;
  setControl(frame.control);
  canvas.style.cursor = control.owned ? cursors[frame.output.cursor_icon] || "default" : "default";
  if (!control.owned) return;
  for (const command of frame.output.commands) {
    if (command.CopyText !== undefined) void copyText(command.CopyText);
    if (command.OpenUrl) {
      const url = new URL(command.OpenUrl.url, location.href);
      if (!["https:", "http:", "mailto:"].includes(url.protocol)) continue;
      if (remote) {
        const opened = window.open(url.href, "_blank", "noopener,noreferrer");
        if (!opened) {
          showNotice("Follow this link in your browser:");
          externalLink.href = url.href;
          externalLink.hidden = false;
        }
      } else void api("open-url", JSON.stringify({ url: url.href })).catch(error => showNotice(error.message));
    }
  }
  if (frame.paste) void pasteText();
  if (frame.output.ime) {
    const rect = frame.output.ime.cursor_rect;
    input.style.left = `${view.x + rect.min.x * view.scale}px`;
    input.style.top = `${view.y + rect.max.y * view.scale}px`;
  }
}

function queue(event) {
  if (!running || !control.owned) return;
  // Collapse consecutive motion events; preserve ordering around button/key events.
  if (event.PointerMoved && events.at(-1)?.PointerMoved) events[events.length - 1] = event;
  else events.push(event);
  if (event.Text || event.Paste || event.Ime || event === "Cut") dirty = true;
}

let painter;
async function tick() {
  if (!running) return;
  const start = performance.now();
  // The owner processes logical input boundaries locally, including terminal keys.
  const batch = events.splice(0, 512);
  try {
    inFlight = api("frame", JSON.stringify({
      size: [Math.max(1, canvas.clientWidth), Math.max(1, canvas.clientHeight)],
      pixels_per_point: Math.min(4, Math.max(0.5, devicePixelRatio || 1)),
      focused: document.hasFocus(), events: batch, epoch: control.epoch,
    }));
    const response = await inFlight;
    if (!running) return;
    if (response.status !== 204) {
      const frame = await response.json();
      if (frame.control.epoch >= control.epoch) {
        painter.paint(frame);
        platform(frame);
      }
      // Inputs queued during the request have not been reflected in frame.dirty yet.
      if (events.some(event => event.Text || event.Paste || event.Ime || event.Key?.pressed)) dirty = true;
    }
    timer = setTimeout(tick, events.length ? 0 : document.hidden ? 1000 : Math.max(0, 33 - (performance.now() - start)));
  } catch (error) {
    if (!running) return;
    running = false;
    // Never replay a batch: the host may already have applied its actions.
    showNotice(`${error.message}\nReconnect to this editor.`, true);
  }
}

async function connect() {
  if (running) return;
  reconnect.hidden = true;
  try {
    if (!remote && !token) throw new Error("Open the complete URL printed by focus-editor, including its # fragment.");
    if (!painter) painter = new Painter(canvas);
    setControl(await (await api("connect")).json());
    painter.reset();
    events = [];
    running = true;
    notice.hidden = true;
    input.focus({ preventScroll: true });
    void tick();
  } catch (error) { showNotice(error.message, true); }
}

function modifiers(event) {
  const altGraph = event.getModifierState?.("AltGraph") || false;
  return { alt: event.altKey && !altGraph, ctrl: event.ctrlKey && !altGraph, shift: event.shiftKey,
    mac_cmd: isMac && event.metaKey, command: isMac ? event.metaKey : event.ctrlKey && !altGraph };
}

const punctuation = {
  " ": "Space", ":": "Colon", ",": "Comma", "\\": "Backslash", "/": "Slash", "|": "Pipe",
  "?": "Questionmark", "!": "Exclamationmark", "[": "OpenBracket", "]": "CloseBracket",
  "{": "OpenCurlyBracket", "}": "CloseCurlyBracket", "`": "Backtick", "-": "Minus", ".": "Period",
  "+": "Plus", "=": "Equals", ";": "Semicolon", "'": "Quote",
};

function keyName(key, code) {
  if (/^[a-z]$/i.test(key)) return key.toUpperCase();
  if (/^[0-9]$/.test(key)) return `Num${key}`;
  if (/^F([1-9]|[12][0-9]|3[0-5])$/.test(key)) return key;
  if (["ArrowDown", "ArrowLeft", "ArrowRight", "ArrowUp", "Escape", "Tab", "Backspace", "Enter",
    "Insert", "Delete", "Home", "End", "PageUp", "PageDown"].includes(key)) return key;
  return punctuation[key] || (/^Key[A-Z]$/.test(code) ? code.slice(3) : null);
}

function keyboard(event, pressed) {
  if (!running || !control.owned || (event.target !== input && event.target !== canvas)) return;
  const mods = modifiers(event);
  queue({ ModifiersChanged: mods });
  if (composing || event.isComposing || event.keyCode === 229) return;
  const key = keyName(event.key, event.code);
  // Let the browser deliver paste through ClipboardEvent, with its actual clipboard contents.
  if (mods.command && key === "V") return;
  if (pressed && mods.command && !mods.shift && (key === "C" || key === "X")) {
    queue(key === "C" ? "Copy" : "Cut");
    event.preventDefault();
    return;
  }
  if (key) queue({ Key: { key, physical_key: null, pressed, repeat: event.repeat, modifiers: mods } });
  if (mods.command || ["Tab", "Enter", "Backspace", "Delete", "Escape", "ArrowDown", "ArrowUp", "ArrowLeft",
    "ArrowRight", "Home", "End", "PageUp", "PageDown", "Insert"].includes(key) || /^F\d+$/.test(key || "")) {
    event.preventDefault();
  }
}
document.addEventListener("keydown", event => keyboard(event, true));
document.addEventListener("keyup", event => keyboard(event, false));

input.addEventListener("beforeinput", event => {
  if (composing || event.isComposing) return;
  if (event.data && event.data === lastComposition) {
    lastComposition = null;
    event.preventDefault();
    return;
  }
  if (event.inputType === "insertText" && event.data) { queue({ Text: event.data }); event.preventDefault(); }
});
input.addEventListener("input", () => { if (!composing) input.value = ""; });
input.addEventListener("compositionstart", () => { composing = true; });
input.addEventListener("compositionupdate", event => {
  queue({ Ime: { Preedit: { text: event.data, active_range_chars: null } } });
});
input.addEventListener("compositionend", event => {
  queue({ Ime: { Commit: event.data } });
  lastComposition = event.data;
  setTimeout(() => { lastComposition = null; }, 0);
  composing = false;
  input.value = "";
});
document.addEventListener("paste", event => {
  if (!running || !control.owned || (event.target !== input && event.target !== canvas)) return;
  queue({ Paste: event.clipboardData.getData("text/plain") });
  event.preventDefault();
});

function position(event) {
  const rect = canvas.getBoundingClientRect();
  // Only the controller sends pointers, together with its own viewport dimensions.
  return { x: event.clientX - rect.left, y: event.clientY - rect.top };
}
canvas.addEventListener("pointermove", event => queue({ PointerMoved: position(event) }));
canvas.addEventListener("pointerdown", event => {
  if (!running || !control.owned) return;
  input.focus({ preventScroll: true });
  canvas.setPointerCapture(event.pointerId);
  queue({ PointerMoved: position(event) });
  const button = ["Primary", "Middle", "Secondary", "Extra1", "Extra2"][event.button];
  if (button) queue({ PointerButton: { pos: position(event), button, pressed: true, modifiers: modifiers(event) } });
  event.preventDefault();
});
canvas.addEventListener("pointerup", event => {
  const button = ["Primary", "Middle", "Secondary", "Extra1", "Extra2"][event.button];
  if (button) queue({ PointerButton: { pos: position(event), button, pressed: false, modifiers: modifiers(event) } });
  if (canvas.hasPointerCapture(event.pointerId)) canvas.releasePointerCapture(event.pointerId);
});
canvas.addEventListener("pointerleave", () => queue("PointerGone"));
canvas.addEventListener("pointercancel", event => {
  queue({ PointerButton: { pos: position(event), button: "Primary", pressed: false, modifiers: modifiers(event) } });
  queue("PointerGone");
});
canvas.addEventListener("contextmenu", event => event.preventDefault());
canvas.addEventListener("wheel", event => {
  queue({ MouseWheel: { unit: ["Point", "Line", "Page"][event.deltaMode] || "Point",
    delta: { x: -event.deltaX, y: -event.deltaY }, phase: "Move", modifiers: modifiers(event) } });
  event.preventDefault();
}, { passive: false });
window.addEventListener("blur", () => {
  queue({ ModifiersChanged: { alt: false, ctrl: false, shift: false, mac_cmd: false, command: false } });
  queue({ WindowFocused: false });
});
window.addEventListener("focus", () => queue({ WindowFocused: true }));
window.addEventListener("beforeunload", event => {
  if (dirty && !remote) {
    void api("close-prompt", "", true).catch(() => {});
    event.preventDefault();
    event.returnValue = "";
  }
});
window.addEventListener("pagehide", () => {
  if (running) void api("disconnect", "", true).catch(() => {});
  running = false;
  clearTimeout(timer);
});
window.addEventListener("pageshow", event => { if (event.persisted) void connect(); });
canvas.addEventListener("webglcontextlost", event => {
  event.preventDefault();
  running = false;
  clearTimeout(timer);
  showNotice("Browser graphics were interrupted. Reload this tab to reconnect to the editor.");
});
claimButton.addEventListener("click", async () => {
  if (!running) return;
  try {
    setControl(await (await api("claim")).json());
    input.focus({ preventScroll: true });
  } catch (error) { showNotice(error.message); }
});
reconnect.addEventListener("click", () => void connect());
// Nonfatal clipboard/link notices can be dismissed with Escape or a canvas click.
canvas.addEventListener("pointerdown", () => { if (running) notice.hidden = true; });
void connect();
