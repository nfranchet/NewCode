const list = document.querySelector('#editors');
const frame = document.querySelector('#view');
const empty = document.querySelector('#empty');
const status = document.querySelector('#status');
let selected = location.hash.slice(1), records = [];
function select(id) {
  if (selected !== id || !frame.getAttribute('src')) frame.src = `/editors/${id}/`;
  selected = id;
  history.replaceState(null, '', `#${id}`);
  frame.hidden = false; empty.hidden = true;
  for (const button of list.children) button.setAttribute('aria-current', String(button.dataset.id === id));
}
async function refresh() {
  try {
    const response = await fetch('/api/editors', {cache:'no-store'});
    if (!response.ok) throw new Error('Could not load editors');
    records = await response.json();
    list.replaceChildren();
    for (const record of records) {
      const button = document.createElement('button');
      button.dataset.id = record.id; button.title = record.workspace || record.title;
      button.setAttribute('aria-current', String(record.id === selected));
      const title = document.createElement('strong'); title.textContent = record.title.replace(/^Focus Editor — /, '');
      const detail = document.createElement('small'); detail.textContent = `${record.mode === 'native' ? 'Native' : 'App window'} · ${record.id.slice(0,6)}`;
      button.append(title, detail); button.addEventListener('click', () => select(record.id)); list.append(button);
    }
    status.textContent = records.length ? `${records.length} editor${records.length === 1 ? '' : 's'} running` : 'No editors are open';
    if (!records.some(record => record.id === selected)) {
      frame.removeAttribute('src'); frame.hidden = true; empty.hidden = false; selected = '';
      if (records.length) select(records[0].id);
    } else if (!frame.getAttribute('src')) select(selected);
  } catch (error) { status.textContent = error.message; }
  setTimeout(refresh, 1000);
}
void refresh();
