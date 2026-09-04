#!/usr/bin/env node
// Captures the README's screenshots from this client's own UI, drawn with
// its fixtures (the same data the preview and the self-check use): the
// Workbench pages, the setup assistant, the popover's pages and the mini
// window's layouts, each in both appearances. Every picture is this app's
// web view, so the window chrome is the capture's flat backdrop, not the
// platform's; the native repository's screenshots are of real windows.
//
//   pnpm screenshots            # writes ../../docs/screenshots/*.png
//
// Needs Google Chrome (or set CHROME to another Chromium binary).
import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";

const here = dirname(fileURLToPath(import.meta.url));
const app = resolve(here, "..");
const out = resolve(app, "../../docs/screenshots");
const port = Number(process.env.PORT ?? 5197);
const chrome = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
mkdirSync(out, { recursive: true });

// name = file stem (a light capture gets -light); the dark twin has no suffix.
const WORKBENCH = [
  ["workbench-usage", "/?page=usageStats"],
  ["workbench-sessions", "/?page=sessionManager"],
  ["workbench-resets", "/?page=resets"],
  ["workbench-skills", "/?page=skillsManager"],
  ["settings-system", "/?page=settings&section=system"],
  ["settings-costData", "/?page=settings&section=costData"],
  ["onboarding-welcome", "/?page=settings&assistant=1"],
];
const POPOVER = [
  ["popover-overview", "overview"],
  ["popover-openai", "openAI"],
  ["popover-anthropic", "claude"],
  ["popover-google", "googleAI"],
  ["popover-spacexai", "grok"],
  ["popover-misc", "misc"],
  ["popover-machines", "machines"],
];
const MINI = ["regular", "compact", "ledger", "tile", "focus", "rail", "strip:roomy", "strip:twoLine", "strip:narrow"];

const server = spawn("pnpm", ["exec", "vite", "--host", "127.0.0.1", "--port", String(port), "--strictPort", "--clearScreen", "false"], { cwd: app, stdio: ["ignore", "pipe", "pipe"] });
const ready = new Promise((resolveReady, reject) => {
  const timer = setTimeout(() => reject(new Error("vite did not start")), 30_000);
  server.stdout.on("data", (chunk) => {
    if (String(chunk).includes("Local:")) {
      clearTimeout(timer);
      resolveReady();
    }
  });
  server.on("exit", (code) => reject(new Error(`vite exited with ${code}`)));
});

async function capture(browser, { name, url, dark, selector, width = 1180, height = 820 }) {
  const context = await browser.newContext({
    viewport: { width, height },
    deviceScaleFactor: 2,
    colorScheme: dark ? "dark" : "light",
  });
  const page = await context.newPage();
  await page.goto(`http://127.0.0.1:${port}${url}${url.includes("?") ? "&" : "?"}appearance=${dark ? "dark" : "light"}`, { waitUntil: "networkidle" });
  await page.waitForTimeout(400);
  const file = resolve(out, `${name}${dark ? "" : "-light"}.png`);
  if (selector) {
    await page.locator(selector).first().screenshot({ path: file });
  } else {
    await page.screenshot({ path: file });
  }
  await context.close();
  return file;
}

try {
  await ready;
  const browser = await chromium.launch({ executablePath: chrome, headless: true });
  const jobs = [];
  for (const dark of [false, true]) {
    for (const [name, url] of WORKBENCH) jobs.push({ name, url, dark });
    for (const [name, page] of POPOVER) jobs.push({ name, url: `/preview.html?surface=popover&page=${page}`, dark, selector: "#capture", width: 900, height: 1400 });
    for (const layout of MINI) jobs.push({ name: `mini-${layout.replace(":", "-")}`, url: `/preview.html?surface=mini&layout=${encodeURIComponent(layout)}`, dark, selector: "#capture", width: 1200, height: 900 });
  }
  for (const job of jobs) {
    const file = await capture(browser, job);
    console.log(file.replace(`${out}/`, ""));
  }
  await browser.close();
} finally {
  server.kill();
}
