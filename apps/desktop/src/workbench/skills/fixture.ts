/** A synthetic skills inventory for `/preview.html?surface=skills`. */
import type { SkillsInventoryView } from "../../api";
import { FIXTURE_NOW } from "../../popover/fixture";

const all = ["codex", "claude", "gemini", "antigravity", "grok", "cursor"];
const noAntigravity = all.filter((t) => t !== "antigravity");
function skill(name: string, description: string, targets: string[], health = "healthy") {
  const apps = Object.fromEntries(
    ["codex", "claude", "gemini", "antigravity", "grok", "cursor"].map((app) => [app, { state: targets.includes(app) ? "projected" : "missing", adopted: false } as const]),
  );
  return { name, directory: `/Users/example/.agents/skills/${name}`, description, targets, health, source: "local", id: `local:${name}`, registered: true, apps };
}

export const FIXTURE_SKILLS: SkillsInventoryView = {
  scannedAt: FIXTURE_NOW - 40,
  warnings: ["ignored foreign or dangling link: cursor/old-skill"],
  skills: [
    skill("agents-sdk", "Build AI agents on Cloudflare Workers using the Agents SDK. Load when creating stateful agents, durable workflows, real-time WebSocket apps, scheduled tasks, MCP servers, chat applications, voice agents, or browser automation.", all),
    skill("cloudflare", "Comprehensive Cloudflare platform skill covering Workers, Pages, storage (KV, D1, R2), AI (Workers AI, Vectorize, Agents SDK), networking (Tunnel, Spectrum), security (WAF, DDoS), and infrastructure-as-code. Biases towards retrieval from Cloudflare docs over pre-trained knowledge.", all),
    skill("cloudflare-email-service", "Send and receive transactional emails with Cloudflare Email Service (Email Sending + Email Routing). Use when building email sending, email routing, Agents SDK email handling, or integrating email into any app.", noAntigravity),
    skill("code-review", "Run an extremely strict maintainability review for abstraction quality, giant functions, duplicated logic, and naming. Produces a ranked list of findings with the line they anchor to.", all),
    skill("dataviz", "Use before creating any chart, graph, plot or dashboard in any medium. A form heuristic, a colour formula with a runnable validator, mark specs and interaction rules that read as one system in light and dark.", noAntigravity),
    skill("docx", "Create, read, edit and manipulate Word documents (.docx) and templates (.dotx): tables of contents, headings, page numbers, tracked changes, comments, and converting content into a polished document.", ["codex", "claude", "grok", "cursor"]),
    skill("durable-objects", "Create and review Cloudflare Durable Objects. Use when building stateful coordination (chat rooms, multiplayer games, booking systems), implementing RPC methods, SQLite storage, alarms, WebSockets, or reviewing DO code for best practices.", all),
    skill("frontend-design", "Guidance for distinctive, intentional visual design when building new UI or reshaping an existing one. Helps with aesthetic direction, typography, and making choices that don't read as templated defaults.", all),
    skill("gh-address-comments", "Help address review and issue comments on the open GitHub PR for the current branch: fetch the threads, group them by file, apply the change or reply with a reason, and resolve what was handled.", noAntigravity),
    skill("broken-skill", "", [], "unreadable"),
  ],
};
