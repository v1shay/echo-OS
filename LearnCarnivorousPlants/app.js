'use strict';
(function () {

  /* ── Shared constants ──────────────────────────────────────────── */
  const BASE = 'https://commons.wikimedia.org/w/index.php?title=Special:Redirect/file/';
  const COMMONS_API = 'https://commons.wikimedia.org/w/api.php?action=query&prop=imageinfo&iiprop=extmetadata&format=json&origin=*&titles=File:';

  const PLANT_PROFILES = {
    vft: {
      name: 'Venus Flytrap',
      latin: 'Dionaea muscipula',
      quizBlurb: 'You are precise, reactive, and never waste energy on the wrong target. You look dramatic, but your real strength is timing.',
      traits: ['bold', 'direct', 'high light', 'beginner friendly'],
      care: {
        humidity: ['medium', 'high'],
        light: ['high'],
        windows: ['south', 'west'],
        locations: ['north-carolina', 'florida', 'california', 'texas'],
        difficulty: 'easy',
        fitCopy: 'Best when you can give strong direct sun and classic tray-style bog care.'
      }
    },
    sundew: {
      name: 'Sundew',
      latin: 'Drosera spp.',
      quizBlurb: 'You look soft, but you are relentless. People underestimate you right up until you have already won.',
      traits: ['magnetic', 'adaptable', 'sticky trap', 'easy grower'],
      care: {
        humidity: ['medium', 'high'],
        light: ['medium', 'high'],
        windows: ['east', 'south', 'west'],
        locations: ['florida', 'california', 'pacific-northwest', 'other'],
        difficulty: 'easy',
        fitCopy: 'A strong indoor option when you have bright light and can keep the pot consistently wet.'
      }
    },
    pitcher: {
      name: 'Pitcher Plant',
      latin: 'Sarracenia / Nepenthes',
      quizBlurb: 'You build the whole environment, then let everything else wander in. Strategic, patient, and a little theatrical.',
      traits: ['strategic', 'showpiece', 'medium challenge', 'varied forms'],
      care: {
        humidity: ['medium', 'high'],
        light: ['medium', 'high'],
        windows: ['east', 'south', 'west'],
        locations: ['california', 'florida', 'texas', 'other'],
        difficulty: 'medium',
        fitCopy: 'Works best in bright rooms, especially if you can offer extra humidity for tropical types.'
      }
    },
    butterwort: {
      name: 'Butterwort',
      latin: 'Pinguicula spp.',
      quizBlurb: 'You are calm, efficient, and weirdly effective. Minimal motion, maximum results.',
      traits: ['compact', 'stylish', 'gnat hunter', 'windowsill ready'],
      care: {
        humidity: ['low', 'medium'],
        light: ['medium'],
        windows: ['east', 'south'],
        locations: ['california', 'new-york', 'midwest', 'other'],
        difficulty: 'easy',
        fitCopy: 'One of the best fits for normal homes with bright windows and average humidity.'
      }
    },
    bladderwort: {
      name: 'Bladderwort',
      latin: 'Utricularia spp.',
      quizBlurb: 'You operate below the surface and move absurdly fast when it matters. Quiet, clever, underestimated.',
      traits: ['hidden genius', 'fast trap', 'humid grower', 'conversation starter'],
      care: {
        humidity: ['medium', 'high'],
        light: ['medium'],
        windows: ['east', 'south', 'west'],
        locations: ['pacific-northwest', 'florida', 'other'],
        difficulty: 'medium',
        fitCopy: 'A good match when you can maintain moisture and decent light without blasting direct sun all day.'
      }
    },
    cobra: {
      name: 'Cobra Lily',
      latin: 'Darlingtonia californica',
      quizBlurb: 'You are rare, intense, and objectively harder to handle than most people realize. The payoff is worth it.',
      traits: ['hard mode', 'iconic', 'cool roots', 'collector energy'],
      care: {
        humidity: ['high'],
        light: ['medium', 'high'],
        windows: ['north', 'east'],
        locations: ['california', 'pacific-northwest'],
        difficulty: 'hard',
        fitCopy: 'Only a real fit if you can keep roots cool and humidity high; impressive, but not forgiving.'
      }
    }
  };

  const QUIZ_QUESTIONS = [
    {
      prompt: 'Your ideal way to solve a problem?',
      options: [
        { text: 'Act fast and end it cleanly.', note: 'Direct and decisive.', scores: { vft: 3, cobra: 1 } },
        { text: 'Set a trap and let the situation come to me.', note: 'Strategic patience.', scores: { pitcher: 3, bladderwort: 1 } },
        { text: 'Work slowly until the result is unavoidable.', note: 'Quiet persistence.', scores: { sundew: 3, butterwort: 2 } }
      ]
    },
    {
      prompt: 'Which room feels most like home?',
      options: [
        { text: 'A blazing sunroom.', note: 'Bright and exposed.', scores: { vft: 2, sundew: 2, pitcher: 1 } },
        { text: 'A crisp windowsill with clean lines.', note: 'Controlled and practical.', scores: { butterwort: 3, bladderwort: 1 } },
        { text: 'A humid jungle corner.', note: 'Dense and atmospheric.', scores: { pitcher: 2, cobra: 2, bladderwort: 1 } }
      ]
    },
    {
      prompt: 'Pick your social style.',
      options: [
        { text: 'Loud entrance, unforgettable exit.', note: 'Main-character energy.', scores: { vft: 2, pitcher: 2 } },
        { text: 'Subtle charm that pulls people in.', note: 'Soft power.', scores: { sundew: 3, butterwort: 1 } },
        { text: 'Mysterious and slightly intimidating.', note: 'Cult classic energy.', scores: { cobra: 3, bladderwort: 2 } }
      ]
    },
    {
      prompt: 'How much upkeep are you honestly willing to handle?',
      options: [
        { text: 'Keep it simple.', note: 'Low drama.', scores: { butterwort: 3, sundew: 2, vft: 1 } },
        { text: 'Some work is fine if the result is worth it.', note: 'Balanced effort.', scores: { pitcher: 3, bladderwort: 2 } },
        { text: 'I respect beautiful high-maintenance things.', note: 'No fear.', scores: { cobra: 4 } }
      ]
    },
    {
      prompt: 'What kind of reputation sounds best?',
      options: [
        { text: 'Classic icon.', note: 'Recognizable for a reason.', scores: { vft: 3 } },
        { text: 'Underrated specialist.', note: 'Niche and effective.', scores: { butterwort: 2, bladderwort: 2 } },
        { text: 'Collector obsession.', note: 'The one enthusiasts talk about.', scores: { pitcher: 2, cobra: 2, sundew: 1 } }
      ]
    }
  ];

  const QUIZ_TIE_BREAK = ['vft', 'sundew', 'butterwort', 'pitcher', 'bladderwort', 'cobra'];

  /* ── localStorage keys ─────────────────────────────────────── */
  const LS_QUIZ = 'cp_quiz_v1';
  const LS_CARE = 'cp_care_v1';
  const LS_TAB  = 'cp_tab_v1';

  function lsGet(key) {
    try { const v = localStorage.getItem(key); return v ? JSON.parse(v) : null; } catch (_) { return null; }
  }
  function lsSet(key, val) {
    try { localStorage.setItem(key, JSON.stringify(val)); } catch (_) {}
  }
  function lsDel(key) {
    try { localStorage.removeItem(key); } catch (_) {}
  }

  /* ── Utility ────────────────────────────────────────────────── */
  function stripHtml(str) {
    if (!str) return '';
    const d = document.createElement('div');
    d.innerHTML = str;
    return (d.textContent || d.innerText || '').trim();
  }

  const citationCache = {};

  async function fetchCitation(filename) {
    if (filename in citationCache) return citationCache[filename];
    try {
      const res  = await fetch(COMMONS_API + encodeURIComponent(decodeURIComponent(filename)));
      const json = await res.json();
      const page = Object.values(json.query.pages)[0];
      const meta = page?.imageinfo?.[0]?.extmetadata ?? {};
      const author  = stripHtml(meta.Artist?.value) || 'Unknown';
      const license = meta.LicenseShortName?.value || '';
      const licUrl  = meta.LicenseUrl?.value || '';
      citationCache[filename] = { author, license, licUrl };
    } catch (_) {
      citationCache[filename] = null;
    }
    return citationCache[filename];
  }

  /* ── Load hero images lazily ────────────────────────────────── */
  function loadHeroImages() {
    document.querySelectorAll('.panel-hero-img[data-src]').forEach(img => {
      const filename = img.dataset.src;
      const skeleton = img.previousElementSibling;
      img.onload  = () => { img.classList.replace('loading', 'loaded'); if (skeleton) skeleton.classList.add('hidden'); };
      img.onerror = () => { if (skeleton) skeleton.classList.add('hidden'); img.style.display = 'none'; };
      img.src = BASE + filename + '&width=700';
      delete img.dataset.src;
    });
  }
  loadHeroImages();

  /* ── Build one image cell ───────────────────────────────────── */
  function makeImgCell(filename) {
    const item = document.createElement('div');
    item.className = 'img-gallery-item';

    const wrap = document.createElement('div');
    wrap.className = 'img-wrap';

    const skeleton = document.createElement('div');
    skeleton.className = 'img-skeleton';
    wrap.appendChild(skeleton);

    const isExternal = filename.startsWith('http');

    const img = document.createElement('img');
    img.className = 'gallery-img loading';
    img.alt = isExternal ? 'Cultivar photo' : filename.replace(/_/g, ' ').replace(/\.[^.]+$/, '');
    img.onload  = () => { img.classList.replace('loading', 'loaded'); skeleton.classList.add('hidden'); };
    img.onerror = () => {
      skeleton.classList.add('hidden');
      wrap.innerHTML = `<div class="img-empty">
        <svg width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <rect x="3" y="3" width="18" height="18" rx="2"/>
          <circle cx="8.5" cy="8.5" r="1.5"/>
          <polyline points="21 15 16 10 5 21"/>
        </svg>
        <span>Image unavailable</span>
      </div>`;
    };
    img.src = isExternal ? filename : (BASE + filename + '&width=600');
    wrap.appendChild(img);
    item.appendChild(wrap);

    const citeEl = document.createElement('p');
    citeEl.className = 'img-citation';
    item.appendChild(citeEl);

    if (isExternal) {
      const host = new URL(filename).hostname.replace('www.', '');
      citeEl.innerHTML = `<a href="${filename}" target="_blank" rel="noopener">${host}</a>`;
    } else {
      citeEl.innerHTML = '<span class="cite-loading">Loading attribution\u2026</span>';
      const pageUrl = 'https://commons.wikimedia.org/wiki/File:' + decodeURIComponent(filename);
      fetchCitation(filename).then(info => {
        if (!info) {
          citeEl.innerHTML = `<a href="${pageUrl}" target="_blank" rel="noopener">Wikimedia Commons</a>`;
          return;
        }
        const licPart = info.license
          ? (info.licUrl
              ? `<a href="${info.licUrl}" target="_blank" rel="noopener">${info.license}</a>`
              : info.license)
          : '';
        citeEl.innerHTML =
          `<strong>${info.author}</strong>` +
          (licPart ? ` &middot; ${licPart}` : '') +
          ` &middot; <a href="${pageUrl}" target="_blank" rel="noopener">Wikimedia Commons</a>`;
      });
    }

    return item;
  }

  /* ── Tab switching ──────────────────────────────────────────── */
  const PANEL_MAP = {
    vft: 'panel-vft', sundew: 'panel-sundew', pitcher: 'panel-pitcher',
    butterwort: 'panel-butterwort', bladderwort: 'panel-bladderwort', cobra: 'panel-cobra'
  };

  function switchToPlant(key) {
    if (!PANEL_MAP[key]) return;
    document.querySelectorAll('.tab-btn').forEach(t => {
      t.classList.remove('active');
      t.setAttribute('aria-selected', 'false');
    });
    document.querySelectorAll('.plant-panel').forEach(p => {
      p.classList.remove('active');
      p.setAttribute('aria-hidden', 'true');
    });
    const btn = document.querySelector(`.tab-btn[data-panel="${key}"]`);
    if (btn) { btn.classList.add('active'); btn.setAttribute('aria-selected', 'true'); }
    const panel = document.getElementById(PANEL_MAP[key]);
    if (panel) { panel.classList.add('active'); panel.setAttribute('aria-hidden', 'false'); }
    document.querySelector('.tabs-section').scrollIntoView({ behavior: 'smooth', block: 'start' });
    lsSet(LS_TAB, key);
  }

  document.querySelectorAll('.tab-btn').forEach(btn => {
    btn.addEventListener('click', () => switchToPlant(btn.dataset.panel));
  });

  // Restore last active tab
  const savedTab = lsGet(LS_TAB);
  if (savedTab && PANEL_MAP[savedTab]) {
    // Switch without scroll on first load
    document.querySelectorAll('.tab-btn').forEach(t => {
      t.classList.remove('active');
      t.setAttribute('aria-selected', 'false');
    });
    document.querySelectorAll('.plant-panel').forEach(p => {
      p.classList.remove('active');
      p.setAttribute('aria-hidden', 'true');
    });
    const btn = document.querySelector(`.tab-btn[data-panel="${savedTab}"]`);
    if (btn) { btn.classList.add('active'); btn.setAttribute('aria-selected', 'true'); }
    const panel = document.getElementById(PANEL_MAP[savedTab]);
    if (panel) { panel.classList.add('active'); panel.setAttribute('aria-hidden', 'false'); }
  } else {
    // Initialise aria state for default (vft)
    document.querySelector('.tab-btn[data-panel="vft"]')?.setAttribute('aria-selected', 'true');
    document.getElementById('panel-vft')?.setAttribute('aria-hidden', 'false');
    document.querySelectorAll('.tab-btn:not([data-panel="vft"])').forEach(t => t.setAttribute('aria-selected', 'false'));
    document.querySelectorAll('.plant-panel:not(#panel-vft)').forEach(p => p.setAttribute('aria-hidden', 'true'));
  }

  /* ── Variety card click ─────────────────────────────────────── */
  document.querySelectorAll('.variety-card[data-imgs]').forEach(card => {
    card.setAttribute('tabindex', '0');
    card.setAttribute('role', 'button');

    function toggleCard() {
      const panel    = this.closest('.plant-panel');
      const imgPane  = panel.querySelector('.variety-img-panel');
      const inner    = imgPane.querySelector('.variety-img-inner');
      const filenames = this.dataset.imgs.split('|').filter(Boolean);
      const label    = this.dataset.label || this.querySelector('.variety-name').textContent;

      if (this.classList.contains('active')) {
        this.classList.remove('active');
        this.setAttribute('aria-expanded', 'false');
        imgPane.classList.remove('open');
        return;
      }

      panel.querySelectorAll('.variety-card').forEach(c => {
        c.classList.remove('active');
        c.setAttribute('aria-expanded', 'false');
      });
      this.classList.add('active');
      this.setAttribute('aria-expanded', 'true');

      inner.innerHTML = '';

      const heading = document.createElement('p');
      heading.className = 'section-title';
      heading.style.alignSelf = 'flex-start';
      heading.textContent = label;
      inner.appendChild(heading);

      const gallery = document.createElement('div');
      gallery.className = 'img-gallery';
      filenames.forEach(f => gallery.appendChild(makeImgCell(f)));
      inner.appendChild(gallery);

      imgPane.classList.add('open');
      setTimeout(() => imgPane.scrollIntoView({ behavior: 'smooth', block: 'nearest' }), 60);
    }

    card.addEventListener('click', toggleCard);
    card.addEventListener('keydown', function (e) {
      if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggleCard.call(this); }
    });
  });

  /* ── "Why this match" breakdown ─────────────────────────────── */
  const HUM_LABELS  = { low: 'dry/low humidity', medium: 'average humidity', high: 'high humidity' };
  const LIGHT_LABELS = { low: 'low/shade light', medium: 'strong indirect light', high: 'direct sun' };
  const WIN_LABELS  = { north: 'North', east: 'East', south: 'South', west: 'West' };

  function buildWhyItems(profile, selections) {
    const care = profile.care;
    const items = [];

    // Location
    if (care.locations.includes(selections.location)) {
      items.push({ label: 'Location', text: `${selections.location.replace(/-/g, ' ')} suits this plant's natural conditions`, level: 'good' });
    } else if (selections.location === 'other' || care.locations.includes('other')) {
      items.push({ label: 'Location', text: 'Your climate is workable with some care adjustments', level: 'ok' });
    } else {
      items.push({ label: 'Location', text: 'Not a natural climate match — extra care and environment control needed', level: 'warn' });
    }

    // Humidity
    if (care.humidity.includes(selections.humidity)) {
      items.push({ label: 'Humidity', text: `${HUM_LABELS[selections.humidity]} is ideal for this plant`, level: 'good' });
    } else if (selections.humidity === 'medium' && (care.humidity.includes('low') || care.humidity.includes('high'))) {
      items.push({ label: 'Humidity', text: 'Medium humidity is borderline — watch it closely', level: 'ok' });
    } else {
      const needed = care.humidity.map(h => HUM_LABELS[h]).join(' or ');
      items.push({ label: 'Humidity', text: `Needs ${needed}; your setup will need adjustment`, level: 'warn' });
    }

    // Light
    if (care.light.includes(selections.light)) {
      items.push({ label: 'Light', text: `${LIGHT_LABELS[selections.light]} is exactly what this plant wants`, level: 'good' });
    } else if (selections.light === 'high' && care.light.includes('medium')) {
      items.push({ label: 'Light', text: 'Your bright light works — provide some mid-day filtering', level: 'ok' });
    } else {
      items.push({ label: 'Light', text: 'Light may be insufficient; a grow-light supplement would help', level: 'warn' });
    }

    // Window
    if (care.windows.includes(selections.window)) {
      items.push({ label: 'Window', text: `${WIN_LABELS[selections.window]}-facing window is a strong match`, level: 'good' });
    } else if (
      (selections.window === 'west'  && care.windows.includes('south')) ||
      (selections.window === 'east'  && care.windows.includes('north')) ||
      (selections.window === 'north' && care.windows.includes('east'))
    ) {
      items.push({ label: 'Window', text: `${WIN_LABELS[selections.window]}-facing is close but not optimal`, level: 'ok' });
    } else {
      items.push({ label: 'Window', text: `${WIN_LABELS[selections.window]}-facing window may not provide enough light in key seasons`, level: 'warn' });
    }

    return items;
  }

  function renderWhyHtml(items) {
    return `<details class="why-details">
      <summary class="why-summary">Why this match?</summary>
      <div class="why-breakdown" role="list">
        ${items.map(item => `<div class="why-item why-${item.level}" role="listitem">
          <span class="why-label">${item.label}</span>
          <span class="why-text">${item.text}</span>
        </div>`).join('')}
      </div>
    </details>`;
  }

  /* ── Jump-to-plant button HTML ─────────────────────────────── */
  function jumpBtn(key, label) {
    return `<button class="jump-btn" type="button" data-jump="${key}" aria-label="Go to ${label} plant guide tab">Go to ${label} tab &rarr;</button>`;
  }

  /* ── Quiz ───────────────────────────────────────────────────── */
  const quizQuestionEl  = document.getElementById('quiz-question');
  const quizOptionsEl   = document.getElementById('quiz-options');
  const quizResultEl    = document.getElementById('quiz-result');
  const quizStepTextEl  = document.getElementById('quiz-step-text');
  const quizProgressEl  = document.getElementById('quiz-progress');
  const quizProgressBar = quizProgressEl?.parentElement;

  let quizIndex  = 0;
  let quizScores = {};

  function saveQuizState(done) {
    lsSet(LS_QUIZ, { index: quizIndex, scores: quizScores, done: !!done });
  }

  function resetQuiz() {
    quizIndex  = 0;
    quizScores = Object.keys(PLANT_PROFILES).reduce((acc, k) => { acc[k] = 0; return acc; }, {});
    quizResultEl.classList.remove('active');
    quizResultEl.innerHTML = '';
    renderQuizQuestion();
    lsDel(LS_QUIZ);
  }

  function buildTraitPills(traits) {
    return traits.map(t => `<span class="trait-pill">${t}</span>`).join('');
  }

  function getQuizWinner() {
    return Object.entries(quizScores)
      .sort((a, b) => b[1] !== a[1] ? b[1] - a[1] : QUIZ_TIE_BREAK.indexOf(a[0]) - QUIZ_TIE_BREAK.indexOf(b[0]))[0][0];
  }

  function showQuizResult() {
    const winnerKey = getQuizWinner();
    const winner    = PLANT_PROFILES[winnerKey];
    const maxScore  = Object.values(quizScores).reduce((a, b) => a + b, 0);
    const winScore  = quizScores[winnerKey];

    quizStepTextEl.textContent = 'Result ready';
    if (quizProgressBar) {
      quizProgressBar.setAttribute('aria-valuenow', '100');
      quizProgressBar.setAttribute('aria-label', 'Quiz complete');
    }
    quizProgressEl.style.width = '100%';
    quizQuestionEl.textContent = 'Your carnivorous plant match';
    quizOptionsEl.innerHTML = '';

    // Score breakdown for all plants (sorted)
    const ranked = Object.entries(quizScores)
      .sort((a, b) => b[1] - a[1])
      .map(([k, s]) => `<span class="why-item why-${k === winnerKey ? 'good' : 'ok'}" role="listitem">
          <span class="why-label">${PLANT_PROFILES[k].name.split(' ')[0]}</span>
          <span class="why-text">${s} pts${k === winnerKey ? ' — winner' : ''}</span>
        </span>`).join('');

    quizResultEl.innerHTML = `
      <div class="match-card">
        <p class="result-label">You got</p>
        <h4 class="match-name">${winner.name}</h4>
        <p class="match-latin">${winner.latin}</p>
        <p class="match-copy">${winner.quizBlurb}</p>
        <div class="match-traits">${buildTraitPills(winner.traits)}</div>
        <details class="why-details">
          <summary class="why-summary">Score breakdown</summary>
          <div class="why-breakdown" role="list">${ranked}</div>
        </details>
        <div class="quiz-result-actions">
          ${jumpBtn(winnerKey, winner.name)}
          <button class="secondary-btn" type="button" id="quiz-restart">Retake Quiz</button>
        </div>
      </div>
    `;
    quizResultEl.classList.add('active');
    quizResultEl.querySelector('#quiz-restart').addEventListener('click', resetQuiz);
    quizResultEl.querySelectorAll('.jump-btn').forEach(btn => {
      btn.addEventListener('click', () => switchToPlant(btn.dataset.jump));
    });

    saveQuizState(true);
  }

  function renderQuizQuestion() {
    const question = QUIZ_QUESTIONS[quizIndex];
    const step     = quizIndex + 1;
    const pct      = Math.round((step / QUIZ_QUESTIONS.length) * 100);

    quizStepTextEl.textContent = `Question ${step} / ${QUIZ_QUESTIONS.length}`;
    quizProgressEl.style.width = `${pct}%`;
    if (quizProgressBar) {
      quizProgressBar.setAttribute('aria-valuenow', String(pct));
      quizProgressBar.setAttribute('aria-label', `Question ${step} of ${QUIZ_QUESTIONS.length}`);
    }
    quizQuestionEl.textContent = question.prompt;
    quizOptionsEl.innerHTML = '';

    question.options.forEach((option, i) => {
      const button = document.createElement('button');
      button.type      = 'button';
      button.className = 'quiz-option';
      button.setAttribute('aria-label', option.text);
      button.innerHTML = `<span class="option-title">${option.text}</span><span class="option-note">${option.note}</span>`;
      button.addEventListener('click', () => {
        Object.entries(option.scores).forEach(([k, v]) => { quizScores[k] += v; });
        if (quizIndex === QUIZ_QUESTIONS.length - 1) {
          showQuizResult();
        } else {
          quizIndex += 1;
          renderQuizQuestion();
          saveQuizState(false);
        }
      });
      quizOptionsEl.appendChild(button);
      // Auto-focus first option for keyboard users
      if (i === 0) setTimeout(() => button.focus(), 50);
    });
  }

  // Restore or initialise quiz
  const savedQuiz = lsGet(LS_QUIZ);
  if (savedQuiz) {
    quizIndex  = savedQuiz.index  || 0;
    quizScores = savedQuiz.scores || Object.keys(PLANT_PROFILES).reduce((acc, k) => { acc[k] = 0; return acc; }, {});
    if (savedQuiz.done) {
      showQuizResult();
    } else {
      renderQuizQuestion();
    }
  } else {
    resetQuiz();
  }

  /* ── Care calculator ─────────────────────────────────────────── */
  const careForm       = document.getElementById('care-form');
  const careResults    = document.getElementById('care-results');
  const careResultList = document.getElementById('care-result-list');
  const careSummary    = document.getElementById('care-summary');
  const careReset      = document.getElementById('care-reset');

  function scoreCareMatch(profile, selections) {
    const reasons = [];
    let score = 0;

    if (profile.care.locations.includes(selections.location)) {
      score += 3;
      reasons.push(`${selections.location.replace(/-/g, ' ')} climate fits`);
    } else if (selections.location === 'other' || profile.care.locations.includes('other')) {
      score += 1;
      reasons.push('location is workable');
    }

    if (profile.care.humidity.includes(selections.humidity)) {
      score += 3;
      reasons.push('humidity fits');
    } else if (selections.humidity === 'medium' && (profile.care.humidity.includes('low') || profile.care.humidity.includes('high'))) {
      score += 1;
      reasons.push('humidity is workable');
    }

    if (profile.care.light.includes(selections.light)) {
      score += 4;
      reasons.push('light level fits');
    } else if (selections.light === 'high' && profile.care.light.includes('medium')) {
      score += 1;
      reasons.push('light can work with care');
    }

    if (profile.care.windows.includes(selections.window)) {
      score += 3;
      reasons.push(`${selections.window} window fits`);
    } else if (
      (selections.window === 'west' && profile.care.windows.includes('south')) ||
      (selections.window === 'east' && profile.care.windows.includes('north'))
    ) {
      score += 1;
      reasons.push('window is close');
    }

    if (profile.care.difficulty === 'easy') {
      score += 1;
      reasons.push('forgiving care');
    }

    return { score, reasons };
  }

  function renderCareResults(selections) {
    const matches = Object.entries(PLANT_PROFILES)
      .map(([key, profile]) => ({ key, profile, ...scoreCareMatch(profile, selections) }))
      .sort((a, b) => b.score - a.score)
      .slice(0, 3);

    careResultList.innerHTML = '';
    matches.forEach(match => {
      const whyItems = buildWhyItems(match.profile, selections);
      const item = document.createElement('div');
      item.className = 'result-item';
      item.innerHTML = `
        <div class="result-name-row">
          <span class="result-name">${match.profile.name}</span>
          <span class="result-score">${match.score}/14 fit</span>
        </div>
        <p>${match.profile.care.fitCopy}</p>
        <div class="reason-list">${match.reasons.map(r => `<span class="reason-pill">${r}</span>`).join('')}</div>
        ${renderWhyHtml(whyItems)}
        ${jumpBtn(match.key, match.profile.name)}
      `;
      careResultList.appendChild(item);
    });

    const best = matches[0];
    careSummary.textContent = `${best.profile.name} is your strongest match for ${selections.location.replace(/-/g, ' ')}, ${selections.humidity} humidity, ${selections.light} light, and a ${selections.window}-facing window.`;
    careResults.classList.add('active');

    careResultList.querySelectorAll('.jump-btn').forEach(btn => {
      btn.addEventListener('click', () => switchToPlant(btn.dataset.jump));
    });

    lsSet(LS_CARE, { selections, shown: true });
  }

  careForm.addEventListener('submit', event => {
    event.preventDefault();
    const fd = new FormData(careForm);
    renderCareResults({
      location: fd.get('location'),
      humidity: fd.get('humidity'),
      light:    fd.get('light'),
      window:   fd.get('window')
    });
  });

  careReset.addEventListener('click', () => {
    careResults.classList.remove('active');
    lsDel(LS_CARE);
  });

  // Restore care calculator state
  const savedCare = lsGet(LS_CARE);
  if (savedCare && savedCare.shown && savedCare.selections) {
    const sel = savedCare.selections;
    // Re-apply selections to form fields
    ['location', 'humidity', 'light', 'window'].forEach(name => {
      const el = careForm.querySelector(`[name="${name}"]`);
      if (el && sel[name]) el.value = sel[name];
    });
    renderCareResults(sel);
  }

})();
