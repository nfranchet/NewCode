const list = document.querySelector('#editors');
const frame = document.querySelector('#view');
const empty = document.querySelector('#empty');
const status = document.querySelector('#status');
const workspaceList = document.querySelector('#workspaces');
const actionError = document.querySelector('#action-error');
const addDialog = document.querySelector('#add-dialog');
const stopDialog = document.querySelector('#stop-dialog');
const waiting = new Set(), stopping = new Set();
let selected = location.hash.slice(1), records = [], workspaces = [], stopId = null;
let editorSnapshot = '', workspaceSnapshot = '';

function error(element, message = '') { element.textContent = message; element.hidden = !message; }
async function api(path, method = 'GET', body) {
  const response = await fetch(path, {method, cache:'no-store', ...(body === undefined ? {} : {body:JSON.stringify(body), headers:{'Content-Type':'application/json'}})});
  const text = await response.text();
  let value;
  try { value = JSON.parse(text); } catch { value = {error:text}; }
  return {response, value};
}
function select(id) {
  if (selected !== id || !frame.getAttribute('src')) frame.src = `/editors/${id}/`;
  selected = id;
  history.replaceState(null, '', `#${id}`);
  frame.hidden = false; empty.hidden = true;
  for (const row of list.children) row.querySelector('.editor-select').setAttribute('aria-current', String(row.dataset.id === id));
}
async function start(workspace) {
  if (waiting.has(workspace.id)) return;
  waiting.add(workspace.id); workspaceSnapshot = ''; render();
  error(actionError);
  try {
    const {response, value} = await api(`/api/workspaces/${workspace.id}/start`, 'POST', {});
    if (!response.ok) throw new Error(value.error || 'Could not start editor');
    if (value.editor_id) { waiting.delete(workspace.id); select(value.editor_id); }
  } catch (failure) { waiting.delete(workspace.id); error(actionError, failure.message); }
  workspaceSnapshot = '';
}
async function stop(id, choice = null) {
  if (stopping.has(id)) return;
  stopping.add(id);
  error(document.querySelector('#stop-error'));
  try {
    const {response, value} = await api(`/api/editors/${id}/stop`, 'POST', {choice});
    if (response.status === 409 && value.dirty) {
      stopId = id;
      document.querySelector('#stop-message').textContent = `Save changes before stopping ${records.find(record => record.id === id)?.title || 'this editor'}?`;
      if (!stopDialog.open) stopDialog.showModal();
    } else if (!response.ok) throw new Error(value.error || 'Could not stop editor');
    else { stopDialog.close(); stopId = null; }
  } catch (failure) { error(stopDialog.open ? document.querySelector('#stop-error') : actionError, failure.message); }
  finally { stopping.delete(id); }
}
function render() {
  const editorKey = JSON.stringify(records);
  if (editorSnapshot !== editorKey) {
    editorSnapshot = editorKey; list.replaceChildren();
    for (const record of records) {
      const row = document.createElement('div'); row.className = 'editor-row'; row.dataset.id = record.id;
      const button = document.createElement('button'); button.className = 'editor-select'; button.title = record.workspace || record.title;
      button.setAttribute('aria-current', String(record.id === selected));
      const title = document.createElement('strong'); title.textContent = record.title.replace(/^Focus Editor — /, '');
      const detail = document.createElement('small'); detail.textContent = `${({native:'Native', app:'App window', headless:'Browser session'})[record.mode] || 'Editor'} · ${record.id.slice(0,6)}`;
      button.append(title, detail); button.addEventListener('click', () => select(record.id)); row.append(button);
      if (record.mode === 'headless') {
        const button = document.createElement('button'); button.className = 'stop-editor'; button.textContent = 'Stop'; button.title = 'Stop this browser session';
        button.addEventListener('click', () => void stop(record.id)); row.append(button);
      }
      list.append(row);
    }
  }
  const workspaceKey = JSON.stringify([workspaces, [...waiting]]);
  if (workspaceSnapshot !== workspaceKey) {
    workspaceSnapshot = workspaceKey; workspaceList.replaceChildren();
    for (const workspace of workspaces) {
      const row = document.createElement('section'); row.className = 'workspace'; row.dataset.workspace = workspace.id;
      const title = document.createElement('strong'); title.textContent = workspace.name;
      const path = document.createElement('small'); path.textContent = workspace.path; path.title = workspace.path;
      const detail = document.createElement('span'); detail.className = 'workspace-state';
      detail.textContent = waiting.has(workspace.id) || workspace.state === 'starting' ? 'Starting…' : ({running:'Running', stopped:'Stopped', unavailable:'Folder unavailable', failed:'Start failed'})[workspace.state];
      row.append(title, path, detail);
      if (workspace.editors.length) {
        for (const id of workspace.editors) {
          const button = document.createElement('button'); button.textContent = workspace.editors.length > 1 ? `Open ${id.slice(0,6)}` : 'Open';
          button.addEventListener('click', () => select(id)); row.append(button);
        }
      } else {
        const button = document.createElement('button'); button.className = 'start-workspace'; button.textContent = 'Start';
        button.disabled = waiting.has(workspace.id) || workspace.launching || workspace.state === 'unavailable';
        button.addEventListener('click', () => void start(workspace)); row.append(button);
      }
      if (workspace.error) { const message = document.createElement('p'); message.className = 'workspace-error'; message.textContent = workspace.error; row.append(message); }
      workspaceList.append(row);
    }
    if (!workspaces.length) { const hint = document.createElement('p'); hint.className = 'subtitle'; hint.textContent = 'Previously opened workspaces will be listed here.'; workspaceList.append(hint); }
  }
  status.textContent = records.length ? `${records.length} editor${records.length === 1 ? '' : 's'} running` : 'No editors are open';
}
async function refresh() {
  try {
    const [editors, known] = await Promise.all([api('/api/editors'), api('/api/workspaces')]);
    if (!editors.response.ok || !known.response.ok) throw new Error('Could not load editors and workspaces');
    records = editors.value; workspaces = known.value;
    for (const id of [...waiting]) {
      const workspace = workspaces.find(workspace => workspace.id === id);
      if (workspace?.editors.length && records.some(record => record.id === workspace.editors[0])) { waiting.delete(id); select(workspace.editors[0]); }
      else if (workspace?.state === 'failed' || workspace?.state === 'unavailable') waiting.delete(id);
    }
    render();
    if (!records.some(record => record.id === selected)) {
      frame.removeAttribute('src'); frame.hidden = true; empty.hidden = false; selected = '';
      if (records.length) select(records[0].id);
    } else if (!frame.getAttribute('src')) select(selected);
  } catch (failure) { status.textContent = failure.message; }
  setTimeout(refresh, 1000);
}
document.querySelector('#add-workspace').addEventListener('click', () => { error(document.querySelector('#add-error')); addDialog.showModal(); document.querySelector('#workspace-path').focus(); });
document.querySelector('#add-cancel').addEventListener('click', () => addDialog.close());
document.querySelector('#add-form').addEventListener('submit', async event => {
  event.preventDefault();
  const button = document.querySelector('#add-submit'); button.disabled = true;
  try {
    const {response, value} = await api('/api/workspaces', 'POST', {path:document.querySelector('#workspace-path').value.trim()});
    if (!response.ok) throw new Error(value.error || 'Could not add workspace');
    addDialog.close(); document.querySelector('#add-form').reset();
  } catch (failure) { error(document.querySelector('#add-error'), failure.message); }
  finally { button.disabled = false; }
});
document.querySelector('#stop-save').addEventListener('click', () => void stop(stopId, 'save'));
document.querySelector('#stop-discard').addEventListener('click', () => void stop(stopId, 'discard'));
document.querySelector('#stop-cancel').addEventListener('click', () => { stopDialog.close(); stopId = null; });
void refresh();
