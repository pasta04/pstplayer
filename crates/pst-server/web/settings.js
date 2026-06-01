// PSTPlayer Server — 設定画面。/api/config を GET → form に流し込み、
// 編集して送信したら PUT で書き戻し + ディスクに保存する。

const $ = (id) => document.getElementById(id);

const refs = {
	form: $('form'),
	status: $('status'),
	reload: $('reload'),
	save: $('save'),
	message: $('message'),
	path: $('path'),
	favs: $('favs').querySelector('tbody'),
	addfav: $('addfav'),
};

let current = null;

async function loadConfig() {
	refs.status.hidden = false;
	refs.status.textContent = '読み込み中…';
	refs.form.hidden = true;
	try {
		const [c, p] = await Promise.all([
			fetch('/api/config').then((r) => r.json()),
			fetch('/api/config/path')
				.then((r) => r.json())
				.catch(() => ({ path: '?' })),
		]);
		current = c;
		refs.path.textContent = p.path ?? '?';
		applyToForm(c);
		refs.status.hidden = true;
		refs.form.hidden = false;
	} catch (e) {
		refs.status.textContent = `読み込み失敗: ${e.message || e}`;
	}
}

function applyToForm(cfg) {
	const f = refs.form;
	set(f, 'peercast.host', cfg.peercast?.host);
	set(f, 'peercast.port', cfg.peercast?.port);
	set(f, 'peercast.auth_user', cfg.peercast?.auth_user ?? '');
	set(f, 'peercast.auth_pass', cfg.peercast?.auth_pass ?? '');
	set(f, 'server.bind', cfg.server?.bind);
	set(f, 'server.public_url', cfg.server?.public_url);
	check(f, 'log.debug', cfg.log?.debug);
	set(f, 'log.dir', cfg.log?.dir);
	check(f, 'recording.enabled', cfg.recording?.enabled);
	set(f, 'recording.dir', cfg.recording?.dir);
	set(f, 'recording.ext', cfg.recording?.ext);
	set(f, 'recording.max_concurrent', cfg.recording?.max_concurrent ?? 0);
	renderFavorites(cfg.favorites?.rules ?? []);
}

function set(f, name, value) {
	const el = f.querySelector(`[name="${name}"]`);
	if (el) el.value = value ?? '';
}

function check(f, name, value) {
	const el = f.querySelector(`[name="${name}"]`);
	if (el) el.checked = Boolean(value);
}

function renderFavorites(rules) {
	refs.favs.innerHTML = '';
	for (const r of rules) addFavRow(r);
}

function addFavRow(rule) {
	const tr = document.createElement('tr');
	const cell = (val, type = 'text') => {
		const td = document.createElement('td');
		const inp = document.createElement('input');
		inp.type = type;
		inp.value = val ?? '';
		td.appendChild(inp);
		return { td, inp };
	};
	const checkbox = (val) => {
		const td = document.createElement('td');
		const inp = document.createElement('input');
		inp.type = 'checkbox';
		inp.checked = Boolean(val);
		td.appendChild(inp);
		return { td, inp };
	};
	const data = {};
	for (const [k, type, val] of [
		['name', 'text', rule.name],
		['channel_name', 'text', rule.channel_name],
		['genre', 'text', rule.genre],
		['desc', 'text', rule.desc],
		['comment', 'text', rule.comment],
	]) {
		const c = cell(val, type);
		tr.appendChild(c.td);
		data[k] = c.inp;
	}
	const pin = checkbox(rule.pin_top);
	tr.appendChild(pin.td);
	data.pin_top = pin.inp;
	const rec = checkbox(rule.auto_record);
	tr.appendChild(rec.td);
	data.auto_record = rec.inp;
	const color = cell(rule.color, 'text');
	tr.appendChild(color.td);
	data.color = color.inp;
	const ops = document.createElement('td');
	ops.className = 'ops';
	const up = document.createElement('button');
	up.type = 'button';
	up.textContent = '↑';
	up.onclick = () => move(tr, -1);
	const dn = document.createElement('button');
	dn.type = 'button';
	dn.textContent = '↓';
	dn.onclick = () => move(tr, 1);
	const del = document.createElement('button');
	del.type = 'button';
	del.textContent = '×';
	del.onclick = () => tr.remove();
	ops.appendChild(up);
	ops.appendChild(dn);
	ops.appendChild(del);
	tr.appendChild(ops);
	tr.dataset.rule = ''; // ID 不要
	tr._data = data;
	refs.favs.appendChild(tr);
}

function move(tr, dir) {
	const sibling = dir < 0 ? tr.previousElementSibling : tr.nextElementSibling;
	if (!sibling) return;
	if (dir < 0) refs.favs.insertBefore(tr, sibling);
	else refs.favs.insertBefore(sibling, tr);
}

function collectFavorites() {
	const rules = [];
	for (const tr of refs.favs.querySelectorAll('tr')) {
		const d = tr._data;
		if (!d) continue;
		rules.push({
			name: d.name.value,
			channel_name: d.channel_name.value,
			genre: d.genre.value,
			desc: d.desc.value,
			comment: d.comment.value,
			pin_top: d.pin_top.checked,
			auto_record: d.auto_record.checked,
			color: d.color.value,
		});
	}
	return rules;
}

function collectFromForm() {
	const f = refs.form;
	const port = Number(f['peercast.port'].value) || 7144;
	const maxCon = Number(f['recording.max_concurrent'].value) || 0;
	// 既存設定をベースに差分を上書き (touched 以外を保ちたい)。
	const next = JSON.parse(JSON.stringify(current ?? {}));
	next.peercast = next.peercast ?? {};
	next.peercast.host = f['peercast.host'].value || 'localhost';
	next.peercast.port = port;
	next.peercast.auth_user = f['peercast.auth_user'].value || null;
	next.peercast.auth_pass = f['peercast.auth_pass'].value || null;
	next.server = next.server ?? {};
	next.server.bind = f['server.bind'].value || '0.0.0.0:8080';
	next.server.public_url = f['server.public_url'].value || '';
	next.log = next.log ?? {};
	next.log.debug = f['log.debug'].checked;
	next.log.dir = f['log.dir'].value || '';
	next.recording = next.recording ?? {};
	next.recording.enabled = f['recording.enabled'].checked;
	next.recording.dir = f['recording.dir'].value || '';
	next.recording.ext = f['recording.ext'].value || '';
	next.recording.max_concurrent = maxCon;
	next.favorites = { rules: collectFavorites() };
	return next;
}

async function save(e) {
	e.preventDefault();
	refs.message.textContent = '保存中…';
	const body = collectFromForm();
	try {
		const r = await fetch('/api/config', {
			method: 'PUT',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify(body),
		});
		const txt = await r.text();
		if (!r.ok) {
			refs.message.textContent = `保存失敗 (${r.status}): ${txt}`;
			return;
		}
		current = JSON.parse(txt);
		refs.message.textContent = '保存しました。';
	} catch (e) {
		refs.message.textContent = `保存失敗: ${e.message || e}`;
	}
}

refs.reload.addEventListener('click', loadConfig);
refs.form.addEventListener('submit', save);
refs.addfav.addEventListener('click', () =>
	addFavRow({
		name: '',
		channel_name: '',
		genre: '',
		desc: '',
		comment: '',
		pin_top: false,
		auto_record: false,
		color: '',
	}),
);

loadConfig();
