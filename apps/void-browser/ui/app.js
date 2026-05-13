const invoke = (command, args = {}) => {
  const tauriInvoke = window.__TAURI_INTERNALS__?.invoke;
  if (!tauriInvoke) {
    throw new Error('Tauri command bridge is unavailable.');
  }
  return tauriInvoke(command, args);
};

const state = {
  snapshot: null,
  draftInputs: new Map(),
  showRawSurface: false,
  autoScrollEvents: true,
};

const railItems = [...document.querySelectorAll('.rail-item')];
const routeInput = document.getElementById('route-input');
const navForm = document.getElementById('nav-form');
const refreshButton = document.getElementById('refresh-button');
const syncButton = document.getElementById('sync-button');
const surfaceRoot = document.getElementById('surface-root');
const surfaceCaption = document.getElementById('surface-caption');
const surfaceTitle = document.getElementById('surface-title');
const surfaceMeta = document.getElementById('surface-meta');
const chromeStatusPill = document.getElementById('chrome-status-pill');
const chromeStatusLabel = document.getElementById('chrome-status-label');
const chromeNodeValue = document.getElementById('chrome-node-value');
const surfaceActiveBadge = document.getElementById('surface-active-badge');
const rawSurfaceToggle = document.getElementById('raw-surface-toggle');
const inspectorSummary = document.getElementById('inspector-summary');
const promptCountBadge = document.getElementById('prompt-count-badge');
const railNodeId = document.getElementById('rail-node-id');
const railNetworkStatus = document.getElementById('rail-network-status');
const railNetworkCopy = document.getElementById('rail-network-copy');
const errorBanner = document.getElementById('error-banner');
const promptsRoot = document.getElementById('prompts-root');
const diagnosticsRoot = document.getElementById('diagnostics-root');
const eventsRoot = document.getElementById('events-root');
const eventsAutoscroll = document.getElementById('events-autoscroll');
const promptsPanel = promptsRoot.closest('.prompts-panel');

navForm.addEventListener('submit', async (event) => {
  event.preventDefault();
  await navigate(routeInput.value.trim());
});

refreshButton.addEventListener('click', async () => {
  await navigate(routeInput.value.trim());
});

syncButton.addEventListener('click', async () => {
  await refreshSnapshot();
});

rawSurfaceToggle.addEventListener('click', () => {
  state.showRawSurface = !state.showRawSurface;
  rawSurfaceToggle.textContent = state.showRawSurface ? 'Hide Raw Surface' : 'View Raw Surface';
  if (state.snapshot) {
    renderSurface(state.snapshot);
  }
});

eventsAutoscroll.addEventListener('change', (event) => {
  state.autoScrollEvents = event.target.checked;
});

railItems.forEach((item) => {
  item.addEventListener('click', async () => {
    const rail = item.dataset.rail;
    if (rail === 'chat') {
      routeInput.value = 'void://chat.void';
      await navigate(routeInput.value);
      return;
    }
    setRailState(item.dataset.rail);
  });
});

async function navigate(route) {
  if (!route) return;
  const snapshot = await invoke('browser_navigate', { route });
  applySnapshot(snapshot);
}

async function refreshSnapshot() {
  const snapshot = await invoke('browser_sync');
  applySnapshot(snapshot);
}

async function dispatchAction(action) {
  const snapshot = await invoke('browser_dispatch_action', {
    payload: {
      action,
      inputState: Object.fromEntries(state.draftInputs.entries()),
    },
  });
  applySnapshot(snapshot);
}

async function resolvePrompt(promptId, allowed) {
  const snapshot = await invoke('browser_resolve_prompt', { promptId, allowed });
  applySnapshot(snapshot);
}

function applySnapshot(snapshot) {
  state.snapshot = snapshot;
  hydrateDraftInputs(snapshot);
  routeInput.value = snapshot.current_uri || routeInput.value;
  renderChrome(snapshot);
  renderSurface(snapshot);
  renderPrompts(snapshot.pending_prompts || []);
  renderDiagnostics(snapshot.diagnostics);
  renderEvents(snapshot.events || []);
  errorBanner.textContent = snapshot.last_error || '';
  errorBanner.classList.toggle('hidden', !snapshot.last_error);
}

function hydrateDraftInputs(snapshot) {
  state.draftInputs.clear();
  const inputState = snapshot.surface?.input_state || {};
  Object.entries(inputState).forEach(([key, value]) => {
    state.draftInputs.set(key, value);
  });
}

function renderChrome(snapshot) {
  const surface = snapshot.surface;
  const route = snapshot.current_uri || surface?.route || routeInput.value;
  const routeDisplay = displayRoute(route);
  const nodeId = deriveNodeId(snapshot);
  const gatewayCount = snapshot.diagnostics?.active_gateways || 0;
  const peerCount = snapshot.diagnostics?.active_peers || 0;
  surfaceTitle.textContent = surface ? `${displaySurfaceName(surface)} · ${routeDisplay}` : 'No active surface';
  if (surfaceCaption) {
    surfaceCaption.textContent = surface ? '' : 'Mount a runtime route to open a distributed surface.';
    surfaceCaption.classList.toggle('hidden', !!surface);
  }
  chromeStatusLabel.textContent = snapshot.last_error ? 'ATTENTION' : 'RUNNING';
  chromeStatusPill.className = snapshot.last_error ? 'running-pill attention-pill' : 'running-pill';
  chromeNodeValue.textContent = displayNodeName(snapshot);
  surfaceActiveBadge.classList.toggle('hidden', !surface);
  inspectorSummary.textContent = snapshot.diagnostics
    ? `${snapshot.diagnostics.active_sessions} sessions · ${peerCount} peers · ${snapshot.events?.length || 0} events`
    : 'no active diagnostics';
  promptCountBadge.textContent = String(snapshot.pending_prompts?.length || 0);
  railNodeId.textContent = shorten(nodeId, 22);
  railNetworkStatus.textContent = peerCount > 0 ? 'Connected' : 'Idle';
  railNetworkCopy.textContent = `${peerCount} peers · ${gatewayCount} gateways`;
  rawSurfaceToggle.disabled = !surface;
  rawSurfaceToggle.textContent = state.showRawSurface ? 'Hide Raw Surface' : 'View Raw Surface';

  surfaceMeta.innerHTML = '';
  const pills = buildMetaPills(snapshot);
  pills.forEach((pill) => surfaceMeta.appendChild(pill));
  setRailState(route, surface);
}

function renderSurface(snapshot) {
  surfaceRoot.innerHTML = '';
  const surface = snapshot.surface;

  if (!surface) {
    const empty = document.createElement('div');
    empty.className = 'empty-state mono';
    empty.textContent = 'Mount a runtime route to project its surface tree here.';
    surfaceRoot.appendChild(empty);
    return;
  }

  if (state.showRawSurface) {
    surfaceRoot.appendChild(renderRawSurface(surface));
    return;
  }

  if (isChatSurface(surface)) {
    surfaceRoot.appendChild(renderChatSurface(surface));
    return;
  }

  const rootNode = renderNode(surface.tree.root, surface);
  rootNode.classList.add('surface-node', 'page');
  surfaceRoot.appendChild(rootNode);
}

function renderNode(node, surface) {
  const kind = String(node.kind || '').toLowerCase();
  switch (kind) {
    case 'page':
    case 'column': {
      const wrapper = document.createElement('div');
      wrapper.className = `surface-node ${kind} generic-stack`;
      node.children.forEach((child) => wrapper.appendChild(renderNode(child, surface)));
      return wrapper;
    }
    case 'row': {
      const row = document.createElement('div');
      row.className = 'surface-node row generic-row';
      node.children.forEach((child) => row.appendChild(renderNode(child, surface)));
      return row;
    }
    case 'text': {
      const text = document.createElement('div');
      text.className = 'surface-node text generic-text';
      const value = resolveNodeValue(node, surface.bindings);
      if (!node.properties?.bind && value && value === value.toUpperCase()) {
        text.classList.add('heading', 'generic-heading');
      }
      text.textContent = value;
      return text;
    }
    case 'input': {
      const id = node.properties?.id || node.node_id;
      const wrapper = document.createElement('div');
      wrapper.className = 'generic-input';
      const label = document.createElement('label');
      label.textContent = prettifyToken(id);
      const input = document.createElement('input');
      input.value = state.draftInputs.get(id) ?? surface.input_state?.[id] ?? '';
      input.placeholder = node.properties?.placeholder || '';
      input.addEventListener('input', (event) => {
        state.draftInputs.set(id, event.target.value);
      });
      wrapper.append(label, input);
      return wrapper;
    }
    case 'button': {
      const button = document.createElement('button');
      button.className = `node-button ${buttonTone(node.properties?.action)}`;
      button.textContent = node.properties?.label || 'Action';
      button.addEventListener('click', async () => {
        await dispatchAction(node.properties?.action || 'surface.refresh');
      });
      return button;
    }
    case 'spacer': {
      const spacer = document.createElement('div');
      spacer.className = 'surface-node spacer';
      const size = Number(node.properties?.size || 1);
      spacer.style.height = `${Math.max(1, size) * 12}px`;
      return spacer;
    }
    default: {
      const unknown = document.createElement('div');
      unknown.className = 'node-card mono';
      unknown.textContent = `unsupported runtime node: ${node.kind}`;
      return unknown;
    }
  }
}

function resolveNodeValue(node, bindings) {
  if (node.properties?.bind) {
    return bindings[node.properties.bind] ?? `<${node.properties.bind}:unbound>`;
  }
  return node.properties?.value || '';
}

function renderPrompts(prompts) {
  promptsRoot.innerHTML = '';
  promptCountBadge.textContent = String(prompts.length);
  promptsPanel?.classList.toggle('has-prompts', prompts.length > 0);
  if (!prompts.length) {
    const empty = document.createElement('div');
    empty.className = 'empty-state mono';
    empty.textContent = 'No pending runtime permission prompts.';
    promptsRoot.appendChild(empty);
    return;
  }

  prompts.forEach((prompt) => {
    const card = document.createElement('div');
    card.className = `prompt-card ${prompt.kind === 'GatewayCapability' ? 'gateway' : ''} ${prompt.kind === 'GatewayTrust' ? 'trust' : ''}`;

    const header = document.createElement('div');
    header.className = 'prompt-header';

    const title = document.createElement('div');
    title.className = 'prompt-title';
    title.textContent = prompt.title;

    const capability = document.createElement('div');
    capability.className = 'prompt-meta';
    capability.textContent = `${prompt.kind}\nCapability: ${prompt.capability}`;

    const description = document.createElement('div');
    description.className = 'prompt-description';
    description.textContent = prompt.description;

    const actions = document.createElement('div');
    actions.className = 'prompt-actions';

    const allow = document.createElement('button');
    allow.className = 'node-button allow';
    allow.textContent = 'Allow';
    allow.addEventListener('click', async () => resolvePrompt(prompt.id, true));

    const deny = document.createElement('button');
    deny.className = 'node-button deny';
    deny.textContent = 'Deny';
    deny.addEventListener('click', async () => resolvePrompt(prompt.id, false));

    actions.append(allow, deny);
    header.append(title, capability);
    card.append(header, description, actions);
    promptsRoot.appendChild(card);
  });
}

function renderDiagnostics(diagnostics) {
  if (!diagnosticsRoot) return;

  const d = diagnostics || {};
  const metrics = [
    { icon: '⬡', label: 'SURFACES',        value: d.mounted_surfaces ?? d.surfaces ?? 2 },
    { icon: '◈', label: 'ACTIVE SESSIONS', value: d.active_sessions ?? '--' },
    { icon: '◎', label: 'ACTIVE PEERS',    value: d.active_peers ?? 0 },
    { icon: '◉', label: 'GATEWAYS',        value: d.active_gateways ?? 1 },
    { icon: '◐', label: 'PERMISSIONS',     value: d.permissions ?? d.pending_permissions ?? 4 },
    { icon: '⌁', label: 'BRIDGE SESSIONS', value: d.bridge_sessions ?? 0 },
    { icon: '⬡', label: 'TOPOLOGY',        value: d.topology_state ?? 'BOOTSTRAPPING', small: true },
    { icon: '↗', label: 'LAST ROUTE',      value: d.last_route ?? 'void://chat.void/', xs: true },
  ];

  diagnosticsRoot.innerHTML = '';
  metrics.forEach(({ icon, label, value, small, xs }) => {
    const card = document.createElement('div');
    card.className = 'diag-card';

    const iconEl = document.createElement('span');
    iconEl.className = 'diag-icon';
    iconEl.textContent = icon;

    const labelEl = document.createElement('span');
    labelEl.className = 'diag-label';
    labelEl.textContent = label;

    const valueEl = document.createElement('span');
    valueEl.className = xs ? 'diag-value diag-value--xs' : small ? 'diag-value diag-value--sm' : 'diag-value';
    valueEl.textContent = String(value);

    card.append(iconEl, labelEl, valueEl);
    diagnosticsRoot.appendChild(card);
  });
}

function renderEvents(events) {
  eventsRoot.innerHTML = '';
  const lines = [...events].slice(-18);
  if (!lines.length) {
    const empty = document.createElement('div');
    empty.className = 'empty-state mono';
    empty.textContent = 'No runtime events bridged yet.';
    eventsRoot.appendChild(empty);
    return;
  }

  lines.forEach((event) => {
    const card = document.createElement('div');
    const tone = eventTone(event.line);
    const { time, tag, body } = parseEventLine(event.line);
    card.className = `event-card mono ${tone}`;
    card.innerHTML = `<span class="event-time">${escapeHtml(time)}</span><span class="event-tag">[${escapeHtml(tag)}]</span><span class="event-body">${escapeHtml(body)}</span>`;
    eventsRoot.appendChild(card);
  });

  if (state.autoScrollEvents) {
    eventsRoot.scrollTop = eventsRoot.scrollHeight;
  }
}

function isChatSurface(surface) {
  return surface?.surface_id === 'chat' || surface?.route === 'void://chat.void';
}

function humanizeSurfaceTitle(surface) {
  if (surface?.tree?.title) return surface.tree.title;
  if (surface?.surface_id === 'chat') return 'VOIDChat';
  return prettifyToken(surface?.surface_id || 'runtime surface');
}

function buildMetaPills(snapshot) {
  const pills = [];
  const surface = snapshot.surface;
  const diagnostics = snapshot.diagnostics || {};
  pills.push(metaPill('◴', 'Surface ID', surface?.surface_id || 'unresolved'));
  pills.push(metaPill('⌁', 'Capabilities', surface?.capabilities?.join(', ') || 'runtime'));
  pills.push(metaPill('◎', 'Session', surface?.session_id || 'pending'));
  if (surface?.owner_peer) pills.push(metaPill('◌', 'Owner', shorten(surface.owner_peer, 18)));
  if (diagnostics.topology_state) pills.push(metaPill('⌘', 'Topology', diagnostics.topology_state));
  return pills;
}

function metaPill(icon, label, value) {
  const pill = document.createElement('div');
  pill.className = 'meta-pill';
  pill.innerHTML = `<span class="meta-icon">${escapeHtml(icon)}</span><strong>${escapeHtml(label)}:</strong> ${escapeHtml(value)}`;
  return pill;
}

function setRailState(routeOrTarget, surface) {
  const target = ['runtime', 'chat', 'gateways', 'peers', 'logs'].includes(routeOrTarget)
    ? routeOrTarget
    : routeOrTarget?.includes('gateway') || surface?.surface_id?.startsWith('gateway:')
      ? 'gateways'
      : routeOrTarget?.includes('peer')
        ? 'peers'
        : routeOrTarget?.includes('log')
          ? 'logs'
          : 'runtime';
  railItems.forEach((item) => {
    item.classList.toggle('active', item.dataset.rail === target);
  });
}

function renderMetricChip(icon, label, value) {
  const chip = document.createElement('div');
  chip.className = 'metric-chip';
  const textual = typeof value === 'string' && (value.length > 3 || /[^\d]/.test(value));
  chip.innerHTML = `
    <div class="metric-icon">${escapeHtml(icon)}</div>
    <div class="metric-content">
      <div class="metric-label">${escapeHtml(label)}</div>
      <div class="metric-value ${textual ? 'textual' : ''}">${escapeHtml(String(value))}</div>
    </div>
  `;
  return chip;
}

function renderRawSurface(surface) {
  const card = document.createElement('div');
  card.className = 'raw-surface-card';
  card.innerHTML = `<pre>${escapeHtml(JSON.stringify(surface, null, 2))}</pre>`;
  return card;
}

function renderChatSurface(surface) {
  const bindings = surface.bindings || {};
  const layout = document.createElement('div');
  layout.className = 'chat-layout';

  layout.append(renderChatDirectory(surface, bindings), renderChatConversation(surface, bindings));
  return layout;
}

function renderChatDirectory(surface, bindings) {
  const controls = collectSurfaceControls(surface.tree.root);
  const directory = document.createElement('section');
  directory.className = 'chat-sidebar-panel';

  const wrapper = document.createElement('div');
  wrapper.className = 'chat-directory';

  wrapper.append(
    renderRoomsSection(bindings),
    renderDirectMessagesSection(bindings),
    renderDirectoryFooter(controls),
  );

  directory.appendChild(wrapper);
  return directory;
}

function collectSurfaceControls(node, controls = { inputs: [], buttons: [] }) {
  const kind = String(node.kind || '').toLowerCase();
  if (kind === 'input') {
    controls.inputs.push({
      id: node.properties?.id || node.node_id,
      placeholder: node.properties?.placeholder || '',
    });
  }
  if (kind === 'button') {
    controls.buttons.push({
      label: node.properties?.label || 'Action',
      action: node.properties?.action || 'surface.refresh',
    });
  }
  (node.children || []).forEach((child) => collectSurfaceControls(child, controls));
  return controls;
}

function renderChatConversation(surface, bindings) {
  const controls = collectSurfaceControls(surface.tree.root);
  const currentRoom = normalizeBindingValue(bindings['chat.current_room'], 'current room=none');
  const contactName = derivePrimaryContact(bindings);
  const panel = document.createElement('section');
  panel.className = 'chat-conversation-shell';

  panel.append(
    renderContactHeader(contactName),
    renderDateSeparator(),
    renderMessageStream(surface, bindings, contactName),
    renderTypingRow(contactName, bindings),
    renderComposer(surface, controls, currentRoom),
  );

  return panel;
}

function renderRoomsSection(bindings) {
  const currentRoom = normalizeBindingValue(bindings['chat.current_room'], 'current room=none').toLowerCase();
  const unread = parseBadgeNumber(bindings['chat.unread_count']);
  const rooms = parseLines(bindings['chat.rooms']).map((line, index) => parseListLine(line, index));
  const section = document.createElement('section');
  section.className = 'directory-section';
  section.appendChild(renderSectionHead('ROOMS', () => focusInput('room')));

  const list = document.createElement('div');
  list.className = 'chat-room-list';
  if (!rooms.length) {
    list.appendChild(renderCompactEmpty('room-item empty', 'No joined rooms'));
  } else {
    rooms.forEach((room, index) => {
      const isActive = currentRoom.includes(room.title.toLowerCase());
      list.appendChild(renderRoomItem(room, isActive, unread && (isActive || index === 0) ? unread : ''));
    });
  }
  section.appendChild(list);
  return section;
}

function renderDirectMessagesSection(bindings) {
  const peers = parseLines(bindings['chat.peers']).map((line, index) => parseListLine(line, index));
  const notifications = parseLines(bindings['chat.notifications']);
  const section = document.createElement('section');
  section.className = 'directory-section';
  section.appendChild(renderSectionHead('DIRECT MESSAGES', () => focusInput('peer_id')));

  const list = document.createElement('div');
  list.className = 'chat-dm-list';
  if (!peers.length) {
    list.appendChild(renderCompactEmpty('dm-item empty', 'No peers observed'));
  } else {
    peers.forEach((peer, index) => {
      list.appendChild(renderDmItem(peer, index === 0 ? notifications.length : ''));
    });
  }
  section.appendChild(list);
  return section;
}

function renderDirectoryFooter(controls) {
  const footer = document.createElement('div');
  footer.className = 'sidebar-footer';

  const addPeer = document.createElement('button');
  addPeer.className = 'ghost-action';
  addPeer.textContent = 'Add Peer';
  addPeer.addEventListener('click', () => focusInput('peer_id'));

  footer.append(addPeer);
  return footer;
}

function renderSectionHead(title, onClick) {
  const head = document.createElement('div');
  head.className = 'section-head';
  head.innerHTML = `<h2 class="section-title">${escapeHtml(title)}</h2>`;
  const button = document.createElement('button');
  button.className = 'list-add-button';
  button.type = 'button';
  button.textContent = '+';
  button.addEventListener('click', onClick);
  head.appendChild(button);
  return head;
}

function renderRoomItem(room, active, badgeValue) {
  const item = document.createElement('div');
  item.className = `room-item ${active ? 'active' : ''}`;
  item.innerHTML = `
    <div class="room-head">
      <div class="room-name"><span class="hash-mark">#</span><span>${escapeHtml(room.title)}</span></div>
      <div class="list-row-meta">
        <span class="list-timestamp">${escapeHtml(room.time)}</span>
        ${badgeValue ? `<span class="list-badge">${escapeHtml(String(badgeValue))}</span>` : ''}
      </div>
    </div>
    <div class="list-preview">${escapeHtml(room.preview)}</div>
  `;
  return item;
}

function renderDmItem(peer, badgeValue) {
  const item = document.createElement('div');
  item.className = 'dm-item';
  item.innerHTML = `
    <div class="dm-head">
      <div class="contact-copy">
        <div class="dm-avatar">${escapeHtml(initials(peer.title))}</div>
        <div>
          <div class="dm-name">${escapeHtml(peer.title)}</div>
          <div class="list-preview">${escapeHtml(peer.preview)}</div>
        </div>
      </div>
      <div class="list-row-meta">
        <span class="list-timestamp">${escapeHtml(peer.time)}</span>
        ${badgeValue ? `<span class="list-badge">${escapeHtml(String(badgeValue))}</span>` : ''}
      </div>
    </div>
  `;
  return item;
}

function renderContactHeader(contactName) {
  const header = document.createElement('div');
  header.className = 'contact-header';
  header.innerHTML = `
    <div class="contact-copy">
      <div class="contact-avatar">${escapeHtml(initials(contactName))}</div>
      <div>
        <div class="contact-name">${escapeHtml(contactName)}</div>
        <div class="contact-status"><span class="online-dot"></span><span>Online</span></div>
      </div>
    </div>
    <div class="contact-actions">
      <button class="icon-button" type="button" aria-label="Search">⌕</button>
      <button class="icon-button" type="button" aria-label="Call">☎</button>
      <button class="icon-button" type="button" aria-label="Info">ⓘ</button>
      <button class="icon-button" type="button" aria-label="More">⋮</button>
    </div>
  `;
  return header;
}

function renderDateSeparator() {
  const separator = document.createElement('div');
  separator.className = 'date-separator';
  separator.textContent = 'HOJE';
  return separator;
}

function renderMessageStream(surface, bindings, contactName) {
  const stream = document.createElement('div');
  stream.className = 'message-stream';
  const messages = parseMessageLines(bindings['chat.inbox_messages'])
    .map((line, index) => parseMessageLine(line, index, surface, contactName));
  if (!messages.length) {
    stream.appendChild(renderElegantEmptyState());
    return stream;
  }

  messages.forEach((message) => {
    const row = document.createElement('div');
    row.className = `message-row ${message.self ? 'self' : ''}`;
    row.innerHTML = `
      <div class="message-avatar">${escapeHtml(initials(message.author))}</div>
      <div class="message-bubble">
        <div class="message-bubble-head">
          <span class="message-author">${escapeHtml(message.author)}</span>
          <span class="message-time">${escapeHtml(message.time)}</span>
        </div>
        <div class="message-body">${escapeHtml(message.body)}</div>
      </div>
    `;
    stream.appendChild(row);
  });
  return stream;
}

function renderTypingRow(contactName, bindings) {
  const row = document.createElement('div');
  row.className = 'typing-row';
  const status = parseTypingStatus(bindings['chat.status'], contactName);
  row.innerHTML = `
    <span class="typing-dots"><span></span><span></span><span></span></span>
    <span class="typing-copy">${escapeHtml(status)}</span>
  `;
  return row;
}

function renderComposer(surface, controls, currentRoom) {
  const wrapper = document.createElement('div');
  wrapper.className = 'composer-shell';
  const messageInputDef = controls.inputs.find((inputDef) => inputDef.id === 'message') || { id: 'message', placeholder: 'Mensagem segura...' };
  const sendAction = controls.buttons.find((buttonDef) => buttonDef.action === 'chat.send');

  seedChatDrafts(surface, currentRoom);

  const row = document.createElement('div');
  row.className = 'composer-row';

  const tools = document.createElement('div');
  tools.className = 'composer-tools';
  ['⌂', '</>', '☺'].forEach((symbol) => {
    const button = document.createElement('button');
    button.className = 'icon-button composer-tool';
    button.type = 'button';
    button.textContent = symbol;
    tools.appendChild(button);
  });

  const input = document.createElement('input');
  input.className = 'composer-input';
  input.dataset.composeId = messageInputDef.id;
  input.value = state.draftInputs.get(messageInputDef.id) ?? surface.input_state?.[messageInputDef.id] ?? '';
  input.placeholder = messageInputDef.placeholder || 'Mensagem segura...';
  input.addEventListener('input', (event) => {
    state.draftInputs.set(messageInputDef.id, event.target.value);
  });
  input.addEventListener('keydown', async (event) => {
    if (event.key === 'Enter' && sendAction) {
      event.preventDefault();
      await dispatchAction(sendAction.action);
    }
  });

  const sendButton = document.createElement('button');
  sendButton.className = 'send-button';
  sendButton.type = 'button';
  sendButton.textContent = '➤';
  sendButton.addEventListener('click', async () => {
    await dispatchAction(sendAction?.action || 'chat.send');
  });

  row.append(tools, input, sendButton);

  const footer = document.createElement('div');
  footer.className = 'composer-footer';
  footer.textContent = '⌁ Mensagens cifradas ponta-a-ponta (AES-GCM)';

  wrapper.append(row, footer);
  return wrapper;
}

function seedChatDrafts(surface, currentRoom) {
  if (!state.draftInputs.has('room')) {
    const cleanRoom = normalizeRoomName(currentRoom);
    if (cleanRoom) state.draftInputs.set('room', cleanRoom);
  }
  if (!state.draftInputs.has('peer_id') && surface.input_state?.peer_id) {
    state.draftInputs.set('peer_id', surface.input_state.peer_id);
  }
}

function renderLineItem(className, title, body) {
  const item = document.createElement('div');
  item.className = className;
  item.innerHTML = `<div class="generic-heading">${escapeHtml(title)}</div><div class="generic-text">${escapeHtml(body)}</div>`;
  return item;
}

function renderCompactEmpty(className, title) {
  const item = document.createElement('div');
  item.className = className;
  item.innerHTML = `<div class="compact-empty">${escapeHtml(title)}</div>`;
  return item;
}

function renderElegantEmptyState() {
  const item = document.createElement('div');
  item.className = 'chat-empty-state';
  item.innerHTML = `
    <div class="chat-empty-icon">✦</div>
    <div class="chat-empty-title">No secure messages yet</div>
    <div class="chat-empty-copy">Mount a room or start a direct session to project conversation state here.</div>
  `;
  return item;
}

function parseLines(value) {
  return String(value || '')
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line && !/^none$/i.test(line));
}

function parseListLine(line, index) {
  const normalized = String(line || '').trim();
  const roomMatch = normalized.match(/^(\S+)\s+joined=(\S+)\s+active_members=(\d+)\s+members=([^\s]*)\s+events=(\d+)/);
  if (roomMatch) {
    const [, roomName, joined, activeMembers, members, events] = roomMatch;
    const memberCount = members && members !== '-' ? members.split(',').filter(Boolean).length : 0;
    return {
      title: cleanToken(roomName),
      preview: `${joined === 'true' ? memberCount || activeMembers : 0} members · ${events} events`,
      time: Number(events) > 0 ? `${events} evt` : 'idle',
    };
  }

  const peerMatch = normalized.match(/^(\S+)\s+(\S+)\s+last_seen=(\d+)\s+sessions=(\S+)/);
  if (peerMatch) {
    const [, peerId, stateLabel, lastSeen, sessionId] = peerMatch;
    return {
      title: cleanToken(peerId),
      preview: `${stateLabel.toLowerCase()} · ${sessionId === '-' ? 'no session' : shorten(sessionId, 12)}`,
      time: formatRelativeTimestamp(lastSeen),
    };
  }

  const segments = normalized.split(' · ').filter(Boolean);
  const title = segments[0] || `item ${index + 1}`;
  const time = extractTime(normalized) || ['Agora', '12:40', '11:32', 'Ontem'][index] || '--';
  const preview = segments.slice(1).join(' · ') || 'live runtime state';
  return { title: cleanToken(title), preview, time };
}

function parseMessageLine(line, index, surface, contactName) {
  const authorFallback = index % 2 === 0 ? contactName : displayNodeName(state.snapshot || { surface });
  const colonMatch = String(line).match(/^([^:]{1,48}):\s*(.+)$/);
  const segments = String(line).split(' · ').filter(Boolean);
  const author = cleanToken(colonMatch?.[1] || segments[0] || authorFallback);
  const body = colonMatch?.[2] || (segments.length > 1 ? segments.slice(1).join(' · ') : line);
  const time = extractTime(line) || '--:--';
  const self = inferSelf(author, index);
  return { author, body, time, self };
}

function parseMessageLines(value) {
  return parseLines(value).filter((line) => !/^(inbox empty|no messages|none)$/i.test(String(line).trim()));
}

function inferSelf(author, index) {
  const nodeName = displayNodeName(state.snapshot || {}).toLowerCase();
  const lower = String(author).toLowerCase();
  if (nodeName && lower.includes(nodeName)) return true;
  if (lower === 'me' || lower === 'self' || lower === 'you') return true;
  return index % 2 === 1;
}

function focusInput(id) {
  const input = document.querySelector(`[data-compose-id="${id}"]`);
  if (input) input.focus();
}

function parseBadgeNumber(value) {
  const match = String(value || '').match(/(\d+)/);
  return match ? Number(match[1]) : 0;
}

function parseTypingStatus(value, contactName) {
  const text = String(value || '').trim();
  if (!text) return `${contactName} is connected`;
  const unreadMatch = text.match(/unread=(\d+)/);
  if (unreadMatch) {
    return unreadMatch[1] === '0'
      ? `${contactName} is online`
      : `${contactName} · ${unreadMatch[1]} unread updates`;
  }
  return `${contactName} is online`;
}

function extractTime(line) {
  return String(line).match(/\b\d{1,2}:\d{2}(?::\d{2})?\b/)?.[0] || '';
}

function derivePrimaryContact(bindings) {
  const peerLine = parseLines(bindings['chat.peers'])[0];
  if (peerLine) return cleanToken(peerLine.split(' · ')[0]);
  const room = normalizeBindingValue(bindings['chat.current_room'], 'runtime peer');
  return cleanToken(room.replace(/current room=?/i, '')) || 'Runtime Peer';
}

function normalizeRoomName(value) {
  const match = String(value || '').match(/current room=(.+)$/i);
  return match ? match[1].trim() : '';
}

function deriveNodeId(snapshot) {
  return snapshot.surface?.owner_peer
    || snapshot.diagnostics?.peers?.[0]?.peer_id
    || 'unresolved-node';
}

function displayNodeName(snapshot) {
  const nodeId = deriveNodeId(snapshot);
  return nodeId.includes('@') ? nodeId : shorten(nodeId, 14);
}

function displaySurfaceName(surface) {
  if (surface?.surface_id === 'chat') return 'CHAT';
  return humanizeSurfaceTitle(surface).toUpperCase();
}

function displayRoute(route) {
  const text = String(route || 'unresolved');
  return /^void:\/\/[^/]+$/.test(text) ? `${text}/` : text;
}

function normalizeBindingValue(value, fallback = '') {
  const text = String(value || '').trim();
  return text || fallback;
}

function parseEventLine(line) {
  const raw = String(line || '');
  const time = raw.match(/\b\d{2}:\d{2}:\d{2}\b/)?.[0] || '--:--:--';
  const tag = detectEventTag(raw);
  const body = raw
    .replace(time, '')
    .replace(/\[[^\]]+\]/g, '')
    .trim() || raw;
  return { time, tag, body };
}

function detectEventTag(line) {
  const upper = String(line).toUpperCase();
  if (upper.includes('SURFACE')) return 'SURFACE';
  if (upper.includes('SESSION')) return 'SESSION';
  if (upper.includes('PERMISSION')) return 'PERMISSION';
  if (upper.includes('GATEWAY')) return 'GATEWAY';
  if (upper.includes('PEER')) return 'PEER';
  if (upper.includes('ROUTE')) return 'ROUTE';
  return 'RUNTIME';
}

function eventTone(line) {
  const tag = detectEventTag(line).toLowerCase();
  if (String(line).toLowerCase().includes('error') || String(line).toLowerCase().includes('failed')) return 'error';
  return tag;
}

function buttonTone(action) {
  if (!action) return '';
  if (action.includes('send') || action.includes('submit')) return 'primary';
  if (action.includes('delete') || action.includes('remove') || action.includes('deny')) return 'deny';
  return '';
}

function cleanToken(value) {
  return String(value || '')
    .replace(/^[#@]+/, '')
    .replace(/^[^a-zA-Z0-9]+/, '')
    .trim() || 'Unknown';
}

function prettifyToken(value) {
  return String(value || '')
    .replace(/[_-]+/g, ' ')
    .replace(/\b\w/g, (char) => char.toUpperCase());
}

function initials(value) {
  const parts = cleanToken(value).split(/\s+/).filter(Boolean);
  return parts.slice(0, 2).map((part) => part[0]?.toUpperCase() || '').join('') || 'V';
}

function shorten(value, max = 18) {
  const text = String(value || '');
  if (text.length <= max) return text;
  return `${text.slice(0, Math.max(4, Math.floor(max / 2) - 1))}...${text.slice(-(Math.max(4, Math.floor(max / 2) - 2)))}`;
}

function formatRelativeTimestamp(unixMs) {
  const numeric = Number(unixMs);
  if (!Number.isFinite(numeric) || numeric <= 0) return '--';
  const deltaMinutes = Math.max(0, Math.round((Date.now() - numeric) / 60000));
  if (deltaMinutes < 1) return 'agora';
  if (deltaMinutes < 60) return `${deltaMinutes}m`;
  const deltaHours = Math.round(deltaMinutes / 60);
  if (deltaHours < 24) return `${deltaHours}h`;
  return `${Math.round(deltaHours / 24)}d`;
}

function escapeHtml(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

(async () => {
  try {
    const snapshot = await invoke('browser_snapshot');
    applySnapshot(snapshot);
    if (!snapshot.surface) {
      await navigate(routeInput.value.trim());
    }
  } catch (error) {
    errorBanner.textContent = String(error);
    errorBanner.classList.remove('hidden');
  }
})();

setInterval(() => {
  refreshSnapshot().catch((error) => {
    errorBanner.textContent = String(error);
    errorBanner.classList.remove('hidden');
  });
}, 1500);