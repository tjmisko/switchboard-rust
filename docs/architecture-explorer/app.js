/* Offline UI. This is an explanatory model; it never connects to the daemon. */
(() => {
  'use strict';
  const $ = s => document.querySelector(s);
  const $$ = s => [...document.querySelectorAll(s)];
  const esc = value => String(value ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  const phases = ['Attention','Blocked','Stopped','Unknown','Working','Delegating','Waiting','Idle'];
  const severity = p => p === 'Attention' ? 2 : ['Blocked','Stopped'].includes(p) ? 1 : 0;
  const statusName = s => ({implemented:'IMPLEMENTED',unwired:'TYPE ONLY',proposed:'PROPOSAL','known-gap':'AUDIT GAP','known-bug':'KNOWN BUG',planned:'PLANNED',decision:'OPEN DECISION'}[s] || s.toUpperCase());
  const statusHTML = status => `<span class="badge ${esc(status)}">${esc(statusName(status))}</span>`;
  const sourceHTML = source => source ? `<a class="source-link" href="${esc(new URL(source.path, window.SB_SOURCE_BASE).href)}#L${Number(source.line)}" target="_blank" rel="noopener">${esc(source.path)}:${Number(source.line)} ↗</a>` : '';
  const highlight = code => {
    const re = /(\/\/[^\n]*|"(?:\\.|[^"\\])*"|\b(?:pub|struct|enum|fn|impl|let|mut|match|if|else|return|use|const|async|await|for|in|type|where|self|true|false)\b|\b[A-Z][A-Za-z0-9_]*\b)/g;
    let out = '', last = 0;
    for (const m of String(code).matchAll(re)) {
      out += esc(code.slice(last, m.index));
      const token = m[0];
      const cls = token.startsWith('//') ? 'syntax-comment' : token.startsWith('"') ? 'syntax-string' : /^[A-Z]/.test(token) ? 'syntax-type' : 'syntax-key';
      out += `<span class="${cls}">${esc(token)}</span>`;
      last = m.index + token.length;
    }
    return out + esc(code.slice(last));
  };
  let tab = 'flow', scenarioIndex = 0, stepIndex = 0, timer = null, selectedType = 'input';
  let reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  let toastTimer;
  function toast(message) {
    $('#toast').textContent = message;
    $('#toast').classList.add('visible');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => $('#toast').classList.remove('visible'), 2400);
  }
  async function copy(id, button) {
    const element = document.getElementById(id);
    if (!element) return;
    const value = element.textContent;
    let copied = false;
    try { await navigator.clipboard.writeText(value); copied = true; } catch (_) {
      const input = document.createElement('textarea');
      input.value = value; input.style.position = 'fixed'; input.style.opacity = '0';
      // Keep the fallback inside the modal when it owns keyboard focus.
      const host = $('#inspector').open ? $('#inspector') : document.body;
      host.append(input); input.select(); copied = document.execCommand('copy'); input.remove();
    }
    if (copied) {
      button.textContent = 'Copied'; setTimeout(() => { button.textContent = 'Copy'; }, 1600);
    } else toast('Select the code and use your usual copy shortcut.');
  }
  document.addEventListener('click', e => {
    const button = e.target.closest('[data-copy]');
    if (button) copy(button.dataset.copy, button);
  });
  function changeTab(next, updateHash = true) {
    if (!['flow','state','types','runtime','evolution'].includes(next)) next = 'flow';
    stopPlayback(); tab = next;
    $$('.page').forEach(el => el.classList.toggle('active', el.id === `page-${tab}`));
    $$('.nav-button').forEach(el => {
      const active = el.dataset.tab === tab;
      el.classList.toggle('active', active);
      if (active) el.setAttribute('aria-current','page'); else el.removeAttribute('aria-current');
    });
    $('#crumb').textContent = {flow:'DATA FLOW',state:'AGENT STATE',types:'TYPE ATLAS',runtime:'CONCURRENCY',evolution:'DESIGN BOUNDARIES'}[tab];
    if (updateHash) history.replaceState(null,'',`#${tab}`);
    if (tab === 'flow') updateMap();
    else $('#system-map').pauseAnimations();
    window.scrollTo({top:0,behavior:'instant'});
  }
  $$('.nav-button').forEach(el => el.addEventListener('click', () => changeTab(el.dataset.tab)));
  window.addEventListener('hashchange', () => changeTab(location.hash.slice(1),false));
  $('#start-tour').addEventListener('click', () => {
    scenarioIndex = 0; stepIndex = 0; $('#scenario-select').value = SB_SCENARIOS[0].id;
    changeTab('flow'); renderStep();
    toast('Use Next to read each hop, or Play flow to follow the animation.');
  });
  $('#help-button').addEventListener('click', () => {
    openInspector(`<div class="eyebrow">A GUIDED MAP, NOT A LIVE MONITOR</div><h2>Follow the data.</h2><p>Choose a scenario, then use Play or the arrow buttons. The bright node is the current step; moving dots trace the route being discussed. Click any node for its source and related Rust types.</p><ul><li><strong>Inside an agent:</strong> change independent facts and watch the phase fold pick the first matching rule.</li><li><strong>The Rust types:</strong> search exact declarations, inspect fields, and copy code.</li><li><strong>Tasks & channels:</strong> pause a subscriber to see the difference between a queue and a latest-value channel.</li><li><strong>The wider design:</strong> compare what runs today with the extensible architecture you intend.</li></ul><p>Rust source links open files in the golden worktree. All example payloads are synthetic. Timelines advance at teaching speed, not daemon speed.</p><p>Keyboard: Tab to controls or diagram nodes; Enter opens a node; Escape closes this inspector. Motion can be reduced using the control at the bottom of the rail.</p>`);
  });
  function applyMotion() {
    document.body.classList.toggle('reduced-motion', reduced);
    $('#motion-toggle').textContent = reduced ? 'Enable motion' : 'Reduce motion';
    if (reduced) $('#system-map').pauseAnimations(); else if (tab === 'flow') $('#system-map').unpauseAnimations();
    $$('.particle').forEach(p => p.style.display = reduced ? 'none' : '');
  }
  $('#motion-toggle').addEventListener('click', () => { reduced = !reduced; applyMotion(); });
  $('#map-size').addEventListener('click', () => {
    const expanded = $('.map-card').classList.toggle('expanded');
    $('#map-size').setAttribute('aria-pressed', String(expanded));
    $('#map-size').setAttribute('aria-label', expanded ? 'Fit diagram' : 'Expand diagram');
    $('#map-size').title = expanded ? 'Fit diagram' : 'Expand diagram';
  });

  // The topology is a readable projection of source calls and channels, not a task graph.
  const positions = {
    hook:[30,80], socket:[215,80], adapters:[405,80], scanner:[30,250], watcher:[30,440],
    inbox:[405,250], core:[605,250], board:[605,360], fold:[805,360], view:[805,250],
    effects:[605,490], publisher:[1000,150], subscriber:[1000,280], history:[1000,490]
  };
  const labels = {
    hook:['Hook forwarder','scrub → JSON line','PROCESS / STDIN'],socket:['RPC listener','decode + resolve pid','TASK / UNIX SOCKET'],
    scanner:['Process scanner','/proc · every 1 s','TASK / DISCOVERY'],watcher:['Transcript tail','JSONL · every 200 ms','TASK / FILE'],
    adapters:['Pure adapters','bytes → Input','FUNCTIONS / NO I/O'],inbox:['Input inbox','mpsc<Input> · 4096','BOUNDED CHANNEL'],
    core:['Core task','apply(input) / tick(now)','ONE BOARD OWNER'],board:['Board','agents + accounts','OWNED STATE'],
    fold:['phase → settle','truth → shown','PURE DERIVATION'],view:['Published view','watch<Arc<View>>','LATEST VALUE'],
    effects:['Effect runner','watchers + history','TASK / I/O'],publisher:['Disk publisher','state.json · 250 ms','TASK / COALESCE'],
    subscriber:['RPC subscriber','full snapshot on change','TASK / EACH CLIENT'],history:['History writer','JSONL · best effort','TASK / APPEND']
  };
  const edgePaths = {
    'hook-socket':['M180 111 H215',195,99],
    'socket-adapters':['M365 111 H405',385,99],
    'scanner-inbox':['M180 281 H405',285,270],
    'watcher-adapters':['M180 471 H200 Q210 471 210 461 V187 Q210 175 222 175 H469 Q480 175 480 163 V142',309,164],
    'adapters-inbox':['M480 142 V250',490,224],
    'inbox-core':['M555 281 H605',580,268],
    'core-board':['M680 312 V360',690,342],
    'board-fold':['M755 391 H805',781,379],
    'fold-view':['M880 360 V312',890,342],
    'core-effects':['M605 296 H581 Q570 296 570 309 V509 Q570 521 582 521 H605',580,463],
    'effects-watcher':['M680 552 V557 Q680 572 665 572 H119 Q105 572 105 558 V502',335,559],
    'effects-history':['M755 521 H1000',879,509],
    'view-publisher':['M955 267 H970 Q980 267 980 254 V194 Q980 181 992 181 H1000',1026,233],
    'view-subscriber':['M955 294 H967 Q977 294 977 304 Q977 311 988 311 H1000',1035,355]
  };
  const shortEdgeLabel = {'hook-socket':'','socket-adapters':'','scanner-inbox':'Processes + Died','watcher-adapters':'JSON + LineCtx','adapters-inbox':'Vec<Input>','inbox-core':'Input','core-board':'mutate','board-fold':'facts','fold-view':'render','core-effects':'Effect','effects-watcher':'WatchTranscript / Forget · feedback','effects-history':'AppendHistory','view-publisher':'250 ms coalesce','view-subscriber':'initial + changes'};
  const svg = $('#system-map');
  function renderMap() {
    const edgeMarkup = SB_SYSTEM.edges.map(e => {
      const key = `${e.from}-${e.to}`, [path,x,y] = edgePaths[key];
      return `<path id="edge-${key}" class="edge ${e.kind}" d="${path}" marker-end="url(#arrow-${e.kind})"><title>${esc(e.label)}</title></path>${shortEdgeLabel[key] ? `<text class="edge-label" x="${x}" y="${y}" text-anchor="middle">${esc(shortEdgeLabel[key])}</text>` : ''}`;
    }).join('');
    svg.innerHTML = `<defs><pattern id="grid" width="22" height="22" patternUnits="userSpaceOnUse"><circle cx="1" cy="1" r=".65" fill="#607484" opacity=".18"/></pattern><filter id="node-shadow" x="-20%" y="-25%" width="140%" height="160%"><feDropShadow dx="0" dy="4" stdDeviation="4" flood-opacity=".18"/></filter><filter id="node-glow" x="-40%" y="-70%" width="180%" height="240%"><feDropShadow dx="0" dy="0" stdDeviation="5" flood-color="#7cdec7" flood-opacity=".17"/></filter>${['event','snapshot','effect','view'].map((k,i)=>`<marker id="arrow-${k}" viewBox="0 0 7 7" refX="6" refY="3.5" markerWidth="5" markerHeight="5" orient="auto-start-reverse"><path d="M0 0 L7 3.5 L0 7" fill="${['#699f8b','#7695bb','#a38abd','#b89a70'][i]}"/></marker>`).join('')}</defs><rect width="1180" height="595" fill="url(#grid)"/><text x="30" y="42" class="map-section-label">OBSERVE</text><text x="405" y="42" class="map-section-label">NORMALIZE</text><text x="605" y="42" class="map-section-label">REDUCE & DERIVE</text><text x="1000" y="42" class="map-section-label">DELIVER</text><rect class="map-band" x="590" y="232" width="382" height="209" rx="13"/><text x="605" y="221" class="map-band-label">WITHIN THE CORE TASK · NO BLOCKING I/O</text>${edgeMarkup}<g id="route-extras"></g><g id="particles"></g>${SB_SYSTEM.nodes.map(n=>{
      const [x,y]=positions[n.id], [title,subtitle,kind]=labels[n.id];
      const model=['board','fold','adapters'].includes(n.id);
      return `<g id="node-${n.id}" class="node ${model?'model':''}" tabindex="0" role="button" aria-label="Inspect ${esc(n.title)}" transform="translate(${x},${y})" data-node="${n.id}"><rect class="node-body" width="150" height="62" rx="8"/><circle class="node-dot" cx="13" cy="15" r="2.5"/><text class="node-kind" x="22" y="18">${esc(kind)}</text><text class="node-title" x="12" y="36">${esc(title)}</text><text class="node-subtitle" x="12" y="51">${esc(subtitle)}</text><text class="node-corner" x="133" y="18">↗</text><title>${esc(n.summary)}</title></g>`;
    }).join('')}`;
    $$('.node').forEach(el=>{
      el.addEventListener('click',()=>inspectNode(el.dataset.node));
      el.addEventListener('keydown',e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();inspectNode(el.dataset.node);}});
    });
  }
  function updateMap() {
    const step = SB_SCENARIOS[scenarioIndex].steps[stepIndex];
    $$('.node').forEach(n=>n.classList.toggle('active',n.dataset.node===step.node));
    $$('.edge').forEach(e=>e.classList.remove('active'));
    $('#route-extras').innerHTML=''; $('#particles').innerHTML='';
    let motion='';
    for(let i=0;i<step.route.length-1;i++){
      const from=step.route[i],to=step.route[i+1],key=`${from}-${to}`;
      let path=$(`#edge-${key}`),reverse=false;
      if(!path){path=$(`#edge-${to}-${from}`);reverse=Boolean(path);}
      if(!path){
        const [x1,y1]=positions[from],[x2,y2]=positions[to];
        const extra=document.createElementNS('http://www.w3.org/2000/svg','path');
        extra.setAttribute('d',`M${x1+75} ${y1+31} L${x2+75} ${y2+31}`);
        extra.setAttribute('class','edge event active');extra.id=`extra-${key}`;
        $('#route-extras').append(extra);path=extra;
      }
      path.classList.add('active');
      const colors={event:'#7cdec7',snapshot:'#90bdf5',effect:'#b6a3ee',view:'#f0bc72'};
      const kind=Object.keys(colors).find(k=>path.classList.contains(k))||'event';
      motion+=`<circle class="particle" r="3.5" style="fill:${colors[kind]}"><animateMotion dur="2s" begin="${i*.19}s" repeatCount="indefinite" ${reverse?'keyPoints="1;0" keyTimes="0;1" calcMode="linear"':''}><mpath href="#${path.id}"/></animateMotion></circle>`;
    }
    $('#particles').innerHTML=motion;
    svg.setCurrentTime(0);applyMotion();
  }
  function openInspector(html) {
    stopPlayback();$('#inspector-content').innerHTML=html;
    if(!$('#inspector').open)$('#inspector').showModal();
    $('#inspector').scrollTop=0;
  }
  $('#close-inspector').addEventListener('click',()=>$('#inspector').close());
  $('#inspector').addEventListener('click',e=>{if(e.target===$('#inspector')){
    const r=$('#inspector').getBoundingClientRect();if(e.clientX<r.left||e.clientX>r.right)$('#inspector').close();
  }});
  function findType(name){return SB_TYPES.find(t=>t.name.split(/\s*[·/]\s*/).includes(name)||t.name===name)||SB_TYPES.find(t=>t.code.includes(`struct ${name}`)||t.code.includes(`enum ${name}`));}
  function inspectNode(id){
    const n=SB_SYSTEM.nodes.find(n=>n.id===id);
    openInspector(`<div class="card-top"><span class="eyebrow">${esc(n.layer.toUpperCase())}</span>${statusHTML(n.status)}</div><h2>${esc(n.title)}</h2><p>${esc(n.summary)}</p><ul>${n.details.map(d=>`<li>${esc(d)}</li>`).join('')}</ul><div class="type-code"><div class="card-top"><span class="tiny-label">SOURCE EXCERPT / ABRIDGED WIRING</span><button class="copy-button" data-copy="inspector-code">Copy</button></div><pre><code id="inspector-code">${highlight(n.code)}</code></pre></div>${sourceHTML(n.source)}<h3>Data at this boundary</h3><div>${n.types.map(name=>{const t=findType(name);return t?`<button class="type-jump" data-type="${t.id}">${esc(name)} ↗</button>`:`<span class="effect-chip">${esc(name)}</span>`;}).join('')}</div>`);
    $$('#inspector-content [data-type]').forEach(b=>b.addEventListener('click',()=>{selectedType=b.dataset.type;$('#type-search').value='';$('#inspector').close();changeTab('types');renderTypes();}));
  }
  $('#scenario-select').innerHTML=SB_SCENARIOS.map(s=>`<option value="${s.id}">${esc(s.title)}${s.status==='known-gap'?' · audit gap':''}</option>`).join('');
  $('#scenario-select').addEventListener('change',e=>{stopPlayback();scenarioIndex=SB_SCENARIOS.findIndex(s=>s.id===e.target.value);stepIndex=0;renderStep();});
  function stopPlayback(){clearTimeout(timer);timer=null;$('#play').innerHTML='<span aria-hidden="true">▶</span> Play flow';$('#play').setAttribute('aria-label','Play scenario');}
  function scheduleStep(){
    timer=setTimeout(()=>{
      const s=SB_SCENARIOS[scenarioIndex];
      if(stepIndex>=s.steps.length-1){stopPlayback();return;}
      stepIndex++;renderStep();scheduleStep();
    },Number($('#speed').value));
  }
  $('#play').addEventListener('click',()=>{
    if(timer){stopPlayback();return;}
    if(stepIndex===SB_SCENARIOS[scenarioIndex].steps.length-1){stepIndex=0;renderStep();}
    $('#play').innerHTML='<span aria-hidden="true">Ⅱ</span> Pause';$('#play').setAttribute('aria-label','Pause scenario');scheduleStep();
  });
  $('#speed').addEventListener('change',()=>{if(timer){clearTimeout(timer);scheduleStep();}});
  $('#next').addEventListener('click',()=>{stopPlayback();stepIndex=Math.min(stepIndex+1,SB_SCENARIOS[scenarioIndex].steps.length-1);renderStep();});
  $('#previous').addEventListener('click',()=>{stopPlayback();stepIndex=Math.max(stepIndex-1,0);renderStep();});
  $('#reset').addEventListener('click',()=>{stopPlayback();stepIndex=0;renderStep();});
  function setPhase(el,phase){el.textContent=phase;el.className=`phase-${phases.includes(phase)?phase:'Unknown'}`;}
  function renderStep(){
    const scenario=SB_SCENARIOS[scenarioIndex],step=scenario.steps[stepIndex];
    $('#step-counter').textContent=`STEP ${String(stepIndex+1).padStart(2,'0')} / ${String(scenario.steps.length).padStart(2,'0')}`;
    $('#scenario-status').textContent=statusName(scenario.status);$('#scenario-status').className=`badge ${scenario.status}`;
    $('#step-title').textContent=step.title;$('#step-explanation').textContent=step.explanation;
    $('#step-note').textContent=step.note||'';$('#step-note').className=`note ${scenario.status==='known-gap'?'warning':''}`;
    $('#payload-label').textContent=step.wire.label;$('#payload-code').innerHTML=highlight(step.wire.code);
    setPhase($('#truth-phase'),step.state.phase);setPhase($('#shown-phase'),step.state.shown);
    $('#step-state').innerHTML=['turn','tools','gates','children'].map(k=>`<dt>${k}</dt><dd>${esc(step.state[k])}</dd>`).join('');
    $('#step-effects').innerHTML=step.effects.map(e=>`<span class="effect-chip">${esc(e)}</span>`).join('');
    $('#step-track').innerHTML=scenario.steps.map((s,i)=>`<button class="step-dot ${i===stepIndex?'active':i<stepIndex?'past':''}" data-step="${i}" aria-label="Step ${i+1}: ${esc(s.title)}" title="${esc(s.title)}" ${i===stepIndex?'aria-current="step"':''}></button>`).join('');
    $$('.step-dot').forEach(b=>b.addEventListener('click',()=>{stopPlayback();stepIndex=Number(b.dataset.step);renderStep();}));
    $('#previous').disabled=stepIndex===0;$('#next').disabled=stepIndex===scenario.steps.length-1;
    $('#scenario-sources').innerHTML='<span>TRACE SOURCES</span>'+scenario.source.map(sourceHTML).join('');
    updateMap();
  }

  // A deliberately small projection of the Rust fold. verify.mjs compares it with the Rust oracle.
  function phaseFor(a){
    if(a.gate==='human')return{phase:'Attention',rule:0};
    if(a.limit)return{phase:'Blocked',rule:1};
    if(a.activity==='idle'&&a.outcome==='failed')return{phase:'Blocked',rule:2};
    if(a.activity==='idle'&&a.outcome==='interrupted')return{phase:'Stopped',rule:3};
    if(a.gate==='unknown')return{phase:'Unknown',rule:4};
    if(a.activity==='turn')return{phase:'Working',rule:5};
    if(a.activity==='unobserved')return{phase:'Unknown',rule:6};
    if(a.children)return{phase:'Delegating',rule:7};
    if(a.waits)return{phase:'Waiting',rule:8};
    return{phase:'Idle',rule:9};
  }
  function settleFor(truth,cause,confidence,elapsed){
    const immediate=severity(truth)<=severity('Working')||cause==='User'||confidence==='Confirmed';
    const dwell=immediate?0:confidence==='Snapshot'?6:10;
    return{phase:elapsed>=dwell?truth:'Working',pending:elapsed<dwell,dwell};
  }
  window.SB_LAB={phaseFor,settleFor};
  const rules=[['Attention','A human gate anywhere'],['Blocked','Account limit rejected'],['Blocked','Last idle turn failed'],['Stopped','Last idle turn interrupted'],['Unknown','A gate has unknown ownership'],['Working','Activity::Turn'],['Unknown','Activity::Unobserved'],['Delegating','A subagent runs or is pending'],['Waiting','Background work remains'],['Idle','None of the above']];
  function readAxes(){return{activity:$('#axis-activity').value,gate:$('#axis-gate').value,outcome:$('#axis-outcome').value,limit:$('#axis-limit').checked,children:$('#axis-children').checked,waits:$('#axis-waits').checked};}
  function renderLab(){
    const a=readAxes(),truth=phaseFor(a),cause=$('#lab-cause').value,confidence=$('#lab-confidence').value,elapsed=Number($('#lab-time').value),shown=settleFor(truth.phase,cause,confidence,elapsed);
    $('#fold-rules').innerHTML=rules.map(([phase,rule],i)=>`<li class="${i===truth.rule?'winner':i>truth.rule?'after':''}"><div><strong class="phase-${phase}">${phase}</strong><small>${rule}</small></div>${i===truth.rule?'<span class="rule-check">✓</span>':''}</li>`).join('');
    setPhase($('#lab-truth'),truth.phase);setPhase($('#lab-shown'),shown.phase);$('#lab-time-label').textContent=`${elapsed} s`;
    $('#lab-explanation').textContent=shown.pending?`Pending ${truth.phase}: ${shown.dwell-elapsed} s of the ${shown.dwell} s dwell remain. The current display stays Working.`:shown.dwell?`The ${shown.dwell} s dwell has elapsed. Shown promotes to ${truth.phase}.`:`Immediate: ${cause==='User'?'a user-caused observation bypasses dwell':confidence==='Confirmed'?'confirmed evidence bypasses dwell':'this is not an escalation in severity'}.`;
    $('#lab-warning').hidden=!(truth.phase==='Attention'&&confidence!=='Confirmed');
    $('#lab-warning').textContent='Known policy gap: current Shown can promote inferred or snapshot Attention. It does not enforce “Attention requires confirmed evidence.” This lab preserves that behavior.';
  }
  $$('#page-state select,#page-state input').forEach(e=>e.addEventListener('input',renderLab));
  $('#axis-reset').addEventListener('click',()=>{
    $('#axis-activity').value='turn';$('#axis-gate').value='human';$('#axis-outcome').value='completed';
    ['limit','children','waits'].forEach(k=>{$(`#axis-${k}`).checked=false;});
    $('#lab-cause').value='Agent';$('#lab-confidence').value='Confirmed';$('#lab-time').value=0;renderLab();
  });
  function renderTypes(){
    const query=$('#type-search').value.trim().toLowerCase();
    const filtered=SB_TYPES.filter(t=>[t.name,t.summary,t.code,...t.fields.map(f=>f.meaning)].join(' ').toLowerCase().includes(query));
    let group='';
    $('#type-list').innerHTML=filtered.map(t=>{const header=t.group!==group?`<div class="type-group">${esc(t.group)}</div>`:'';group=t.group;return `${header}<button class="type-item ${t.id===selectedType?'active':''}" data-type="${t.id}" ${t.id===selectedType?'aria-current="true"':''}>${esc(t.name)}</button>`;}).join('')||'<p class="small muted">No matching types.</p>';
    $$('#type-list [data-type]').forEach(b=>b.addEventListener('click',()=>{selectedType=b.dataset.type;renderTypes();}));
    const t=SB_TYPES.find(t=>t.id===selectedType)||SB_TYPES[0];
    $('#type-detail').innerHTML=`<div class="card-top"><span class="eyebrow">${esc(t.group.toUpperCase())} / TYPE ${String(SB_TYPES.indexOf(t)+1).padStart(2,'0')}</span>${statusHTML(t.status)}</div><h2>${esc(t.name)}</h2><p>${esc(t.summary)}</p>${sourceHTML(t.source)}<div class="type-code"><div class="card-top"><span class="tiny-label">RUST DECLARATION · DOC COMMENTS STRIPPED</span><button class="copy-button" data-copy="type-code">Copy</button></div><pre><code id="type-code">${highlight(t.code)}</code></pre></div><h3>Read the fields</h3><div class="table-scroll"><table><thead><tr><th>Field / variant</th><th>Rust type</th><th>Meaning</th></tr></thead><tbody>${t.fields.map(f=>`<tr><td><code>${esc(f.name)}</code></td><td><code>${esc(f.type)}</code></td><td>${esc(f.meaning)}</td></tr>`).join('')}</tbody></table></div><h3>Used by</h3><div class="used-by">${t.usedBy.map(x=>`<span>${esc(x)}</span>`).join('')}</div>`;
  }
  $('#type-search').addEventListener('input',renderTypes);
  $('#runtime-boundaries').innerHTML=SB_SYSTEM.boundaries.map(b=>`<article class="panel boundary"><h3>${esc(b.title)}</h3><p>${esc(b.summary)}</p>${b.code?`<pre><code>${highlight(b.code)}</code></pre>`:''}${sourceHTML(b.source)}</article>`).join('');
  $('#gap-grid').innerHTML=SB_SYSTEM.gaps.map(g=>`<article class="panel gap-card"><div class="card-top"><h3>${esc(g.title)}</h3>${statusHTML(g.status)}</div><span class="gap-label">CURRENT GOLDEN</span><p>${esc(g.current)}</p><div class="proposal"><span class="gap-label">DIRECTION / QUESTION TO SETTLE</span><p>${esc(g.proposal)}</p></div>${sourceHTML(g.source)}</article>`).join('');
  const output={snapshot:{schema:'switchboard-rust/1',updated_ms:1789300000000,agents:[{pid:4036,start:9130042,session_id:'s1',name:'switchboard',cwd:'/work/switchboard',phase:'attention',lamp:'red',since_ms:1789299998000,truth:'attention',reason:'wants approval to run Bash',focused:false}]}};
  $('#output-json').innerHTML=highlight(JSON.stringify(output,null,2));
  let watchVersion=0,watchRead=0,watchPaused=false,watchSkipped=0;
  const viewPhase=n=>['Unknown','Working','Attention','Working','Idle'][n%5];
  function renderWatch(message){
    $('#watch-producer').innerHTML=`View #${watchVersion} <small>${viewPhase(watchVersion)}</small>`;
    $('#watch-current').textContent=`Arc<View #${watchVersion}>`;
    $('#watch-consumer').innerHTML=`View #${watchRead} <small>${viewPhase(watchRead)}</small>`;
    $('#watch-pause').textContent=watchPaused?'Resume subscriber':'Pause subscriber';
    $('#watch-log').textContent=message;
  }
  $('#watch-publish').addEventListener('click',()=>{
    watchVersion++;
    if(!watchPaused)watchRead=watchVersion;
    renderWatch(watchPaused?`Published #${watchVersion}. Reader paused at #${watchRead}; only #${watchVersion} is retained in the channel. ${Math.max(0,watchVersion-watchRead-1)} intermediate view(s) already overwritten.`:`Published #${watchVersion}. Subscriber read the full snapshot. The core did not wait for the socket write.`);
    ['#watch-producer','#watch-current',...(!watchPaused?['#watch-consumer']:[])].forEach(s=>{const e=$(s);e.classList.remove('flash');void e.offsetWidth;e.classList.add('flash');});
  });
  $('#watch-pause').addEventListener('click',()=>{
    watchPaused=!watchPaused;
    if(watchPaused){renderWatch(`Reader paused at #${watchRead}. Publish several changes to replace the single channel slot.`);return;}
    watchSkipped=Math.max(0,watchVersion-watchRead-1);watchRead=watchVersion;
    renderWatch(`Resumed at view #${watchRead}. Skipped ${watchSkipped} intermediate snapshot(s). This channel distributes current state, not an event log.`);
  });
  $('#watch-reset').addEventListener('click',()=>{watchVersion=0;watchRead=0;watchPaused=false;watchSkipped=0;renderWatch('Subscriber is reading. Publish a changed view to start.');});
  document.addEventListener('visibilitychange',()=>{if(document.hidden)stopPlayback();});
  renderMap();renderStep();renderLab();renderTypes();changeTab(location.hash.slice(1)||'flow',false);applyMotion();
})();
