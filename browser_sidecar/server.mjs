import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { chromium } from 'playwright-core';

const host = process.env.JARVIS_BROWSER_HOST || '127.0.0.1';
const port = Number(process.env.JARVIS_BROWSER_PORT || '4317');
const chromeExecutable = process.env.JARVIS_CHROME_EXECUTABLE || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const chromeDebugPort = Number(process.env.JARVIS_CHROME_DEBUG_PORT || '9222');
const attachUrl = process.env.JARVIS_CHROME_ATTACH_URL || `http://127.0.0.1:${chromeDebugPort}`;
const profileDir = process.env.JARVIS_BROWSER_PROFILE_DIR || path.resolve('.tooling/browser-profile');

let browser = null;
let context = null;
let page = null;
let mode = 'uninitialized';

function jsonResponse(res, statusCode, payload) {
  res.writeHead(statusCode, { 'content-type': 'application/json' });
  res.end(JSON.stringify(payload));
}

async function readBody(req) {
  const chunks = [];
  for await (const chunk of req) {
    chunks.push(chunk);
  }
  const raw = Buffer.concat(chunks).toString('utf8').trim();
  if (!raw) {
    return {};
  }
  return JSON.parse(raw);
}

async function getActivePage() {
  if (page && !page.isClosed()) {
    return page;
  }

  if (!context) {
    return null;
  }

  const pages = context.pages();
  if (pages.length > 0) {
    page = pages[0];
    return page;
  }

  page = await context.newPage();
  return page;
}

async function describePage() {
  const activePage = await getActivePage();
  if (!activePage) {
    return {
      ok: true,
      ready: false,
      mode,
      browserName: 'Google Chrome',
    };
  }

  const visibleText = await activePage.evaluate(() => {
    const text = document.body ? document.body.innerText || '' : '';
    return text.replace(/\s+/g, ' ').trim().slice(0, 6000);
  });

  return {
    ok: true,
    ready: true,
    mode,
    browserName: 'Google Chrome',
    url: activePage.url(),
    title: await activePage.title(),
    visibleText,
  };
}

async function tryAttach() {
  try {
    const response = await fetch(`${attachUrl}/json/version`);
    if (!response.ok) {
      return false;
    }
    browser = await chromium.connectOverCDP(attachUrl);
    context = browser.contexts()[0] || null;
    if (!context) {
      return false;
    }
    mode = 'attached_existing';
    page = await getActivePage();
    return true;
  } catch (_error) {
    return false;
  }
}

async function launchDedicatedProfile() {
  fs.mkdirSync(profileDir, { recursive: true });
  context = await chromium.launchPersistentContext(profileDir, {
    executablePath: chromeExecutable,
    headless: false,
    args: [
      `--remote-debugging-port=${chromeDebugPort}`,
      '--no-first-run',
      '--no-default-browser-check',
    ],
  });
  page = await getActivePage();
  browser = context.browser();
  mode = 'jarvis_profile';
}

async function ensureSession() {
  if (context) {
    await getActivePage();
    return describePage();
  }

  if (!fs.existsSync(chromeExecutable)) {
    throw new Error(`Chrome executable not found at ${chromeExecutable}`);
  }

  const attached = await tryAttach();
  if (!attached) {
    await launchDedicatedProfile();
  }

  return describePage();
}

async function browserOpen(payload) {
  const targetUrl = payload.url;
  if (!targetUrl) {
    throw new Error('browser_open requires url');
  }
  const activePage = await getActivePage();
  await activePage.goto(targetUrl, { waitUntil: 'domcontentloaded' });
  return describePage();
}

async function browserWaitFor(payload) {
  const activePage = await getActivePage();
  const timeout = Number(payload.timeout_ms || 10000);

  if (payload.url_contains) {
    await activePage.waitForURL(`**${payload.url_contains}**`, { timeout });
  } else if (payload.text) {
    await activePage.getByText(payload.text, { exact: !!payload.exact }).first().waitFor({ timeout });
  } else if (payload.title_contains) {
    await activePage.waitForFunction(
      expected => document.title.toLowerCase().includes(String(expected).toLowerCase()),
      payload.title_contains,
      { timeout }
    );
  } else {
    await activePage.waitForLoadState('domcontentloaded', { timeout });
  }

  return describePage();
}

async function browserClick(payload) {
  const activePage = await getActivePage();
  if (payload.selector) {
    await activePage.locator(payload.selector).first().click({ timeout: Number(payload.timeout_ms || 10000) });
  } else if (payload.text) {
    await activePage.getByText(payload.text, { exact: !!payload.exact }).first().click({ timeout: Number(payload.timeout_ms || 10000) });
  } else {
    throw new Error('browser_click requires selector or text');
  }
  return describePage();
}

async function browserFill(payload) {
  const activePage = await getActivePage();
  const value = payload.text ?? '';
  if (payload.selector) {
    await activePage.locator(payload.selector).first().fill(value, { timeout: Number(payload.timeout_ms || 10000) });
  } else if (payload.label) {
    await activePage.getByLabel(payload.label, { exact: !!payload.exact }).first().fill(value, { timeout: Number(payload.timeout_ms || 10000) });
  } else {
    throw new Error('browser_fill requires selector or label');
  }
  return describePage();
}

async function browserSnapshot(payload) {
  const snapshot = await describePage();
  const maxChars = Number(payload.max_chars || 6000);
  if (snapshot.visibleText) {
    snapshot.visibleText = snapshot.visibleText.slice(0, maxChars);
  }
  return snapshot;
}

async function browserExtractText(payload) {
  const activePage = await getActivePage();
  const maxChars = Number(payload.max_chars || 6000);
  let text;
  if (payload.selector) {
    text = await activePage.locator(payload.selector).first().innerText({ timeout: Number(payload.timeout_ms || 10000) });
  } else {
    text = await activePage.evaluate(() => (document.body ? document.body.innerText || '' : ''));
  }

  const normalized = text.replace(/\s+/g, ' ').trim().slice(0, maxChars);
  return {
    ...(await describePage()),
    extractedText: normalized,
  };
}

async function browserAssert(payload) {
  const snapshot = await describePage();
  const url = String(snapshot.url || '').toLowerCase();
  const title = String(snapshot.title || '').toLowerCase();
  const text = String(snapshot.visibleText || '').toLowerCase();

  const expectations = [];
  if (payload.url_contains) {
    expectations.push(url.includes(String(payload.url_contains).toLowerCase()));
  }
  if (payload.title_contains) {
    expectations.push(title.includes(String(payload.title_contains).toLowerCase()));
  }
  if (payload.text_contains) {
    expectations.push(text.includes(String(payload.text_contains).toLowerCase()));
  }

  const matched = expectations.length > 0 && expectations.every(Boolean);
  return {
    ...snapshot,
    matched,
    asserted: {
      url_contains: payload.url_contains || null,
      title_contains: payload.title_contains || null,
      text_contains: payload.text_contains || null,
    },
  };
}

async function dispatch(pathname, payload) {
  if (pathname === '/browser/attach_or_launch') {
    return ensureSession();
  }
  await ensureSession();
  if (pathname === '/browser/open') {
    return browserOpen(payload);
  }
  if (pathname === '/browser/wait_for') {
    return browserWaitFor(payload);
  }
  if (pathname === '/browser/click') {
    return browserClick(payload);
  }
  if (pathname === '/browser/fill') {
    return browserFill(payload);
  }
  if (pathname === '/browser/snapshot') {
    return browserSnapshot(payload);
  }
  if (pathname === '/browser/extract_text') {
    return browserExtractText(payload);
  }
  if (pathname === '/browser/assert') {
    return browserAssert(payload);
  }
  throw new Error(`Unknown route ${pathname}`);
}

const server = http.createServer(async (req, res) => {
  try {
    if (req.method === 'GET' && req.url === '/health') {
      jsonResponse(res, 200, await describePage());
      return;
    }

    if (req.method !== 'POST' || !req.url) {
      jsonResponse(res, 404, { ok: false, error: 'not_found' });
      return;
    }

    const payload = await readBody(req);
    const result = await dispatch(req.url, payload);
    jsonResponse(res, 200, result);
  } catch (error) {
    jsonResponse(res, 500, {
      ok: false,
      error: error instanceof Error ? error.message : String(error),
      mode,
    });
  }
});

server.listen(port, host, () => {
  process.stdout.write(`jarvis-browser-sidecar listening on http://${host}:${port}\n`);
});
