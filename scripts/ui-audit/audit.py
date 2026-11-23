"""Open every screen at three widths in both themes and report what can be measured:
sideways scroll, things wider than the window, text under 12px, text cut off,
low contrast, small tap targets, overlapping bars and console errors.

usage:  python3 scripts/ui-audit/audit.py [your-phone] [shots]
        your-phone  the owner's number, as given to dev.sh (10 digits)
        shots       also save a screenshot of every screen to NCRM_AUDIT_OUT

See common.py for what this needs. Read the screenshots too: clutter and
hierarchy can't be measured.
"""
import re, sys, collections
from playwright.sync_api import sync_playwright
from common import URL, OUT, MANAGER, FRONT_DESK, sql, allow_more_codes, sign_in

args = sys.argv[1:]
SHOTS = "shots" in args
OWNER = next((a for a in args if a.isdigit()), MANAGER)

allow_more_codes()
student = sql("select id from students where status = 'active' order by name limit 1")
klass = sql("select id from batches where active order by name limit 1")
session = sql("select s.id from sessions s join organizations o on o.id = s.org_id where s.on_date = (now() at time zone o.timezone)::date order by s.starts_at desc limit 1")
receipt = sql("select id from payments order by receipt_no limit 1")
PATHS = ["/", "/attendance", "/students", "/students/new", f"/students/{student}", f"/students/{student}#fees",
         f"/students/{student}#attendance", f"/students/{student}#details", "/students/import", "/fees",
         "/classes", "/classes/new", f"/classes/{klass}", "/classes/plans", "/team", "/branches", "/more"]
if session: PATHS.insert(2, f"/attendance/{session}")
if receipt: PATHS.insert(-6, f"/fees/receipts/{receipt}")

# Runs in the page. Returns [kind, what] pairs.
CHECK = r"""
() => {
  const vw = innerWidth, out = [];
  const vis = (e) => { const r = e.getBoundingClientRect(); const s = getComputedStyle(e); return r.width > 0 && r.height > 0 && s.visibility !== 'hidden' && s.display !== 'none' && !e.closest('[hidden]'); };
  const label = (e) => { let t = (e.getAttribute('aria-label') || e.innerText || e.value || e.id || e.className || e.tagName).toString().trim().replace(/\s+/g, ' ').slice(0, 44); return `${e.tagName.toLowerCase()}"${t}"`; };
  const inShell = (e) => !!e.closest('.sidebar, .topbar, .bottom-nav');
  if (document.documentElement.scrollWidth > vw + 1) out.push(['overflow-x', `page is ${document.documentElement.scrollWidth}px wide in a ${vw}px window`]);
  const parse = (c) => { const m = c.match(/rgba?\(([^)]+)\)/); if (!m) return null; const p = m[1].split(/[, /]+/).map(Number); return [p[0], p[1], p[2], p.length > 3 ? p[3] : 1]; };
  const lum = (c) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); }; return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2]); };
  const bgOf = (e) => { let layers = []; for (let n = e; n; n = n.parentElement) { const c = parse(getComputedStyle(n).backgroundColor); if (c && c[3] > 0) { layers.push(c); if (c[3] >= 0.99) break; } } let base = parse(getComputedStyle(document.documentElement).backgroundColor) || [255,255,255,1]; for (const l of layers.reverse()) base = [l[0]*l[3] + base[0]*(1-l[3]), l[1]*l[3] + base[1]*(1-l[3]), l[2]*l[3] + base[2]*(1-l[3]), 1]; return base; };
  const seen = new Set();
  for (const e of document.querySelectorAll('main *, .sidebar *, .topbar *, .bottom-nav *')) {
    if (!vis(e) || e.closest('.sr-only')) continue;
    const r = e.getBoundingClientRect(), s = getComputedStyle(e);
    if (r.right > vw + 2 && r.left < vw && !inShell(e) && !e.closest('.tabs') && !e.closest('.timetable')) { const k = 'wide' + label(e); if (!seen.has(k)) { seen.add(k); out.push(['wider-than-screen', `${label(e)} ends at ${Math.round(r.right)}px`]); } }
    const own = [...e.childNodes].some(n => n.nodeType === 3 && n.textContent.trim().length > 1);
    if (!own) continue;
    const fs = parseFloat(s.fontSize);
    if (fs < 12) out.push(['tiny-text', `${label(e)} is ${fs}px`]);
    if (e.scrollWidth > e.clientWidth + 1 && ['hidden', 'clip'].includes(s.overflowX) && s.textOverflow !== 'ellipsis') out.push(['clipped-text', label(e)]);
    if (s.textOverflow === 'ellipsis' && e.scrollWidth > e.clientWidth + 1) out.push(['cut-short', label(e)]);
    const fg = parse(s.color); if (!fg) continue; const bg = bgOf(e);
    const f2 = [fg[0]*fg[3] + bg[0]*(1-fg[3]), fg[1]*fg[3] + bg[1]*(1-fg[3]), fg[2]*fg[3] + bg[2]*(1-fg[3])];
    const L1 = lum(f2), L2 = lum(bg), ratio = (Math.max(L1, L2) + 0.05) / (Math.min(L1, L2) + 0.05);
    const big = fs >= 24 || (fs >= 18.66 && parseInt(s.fontWeight) >= 700);
    const dim = parseFloat(s.opacity) < 1 || e.closest('[disabled]');
    if (!dim && ratio < (big ? 3 : 4.5)) { const k = 'c' + label(e); if (!seen.has(k)) { seen.add(k); out.push(['low-contrast', `${label(e)} ${ratio.toFixed(1)}:1`]); } }
  }
  if (vw < 900) for (const e of document.querySelectorAll('main a, main button, main select, main input, main summary, main [role=tab], .topbar button, .bottom-nav a')) {
    if (!vis(e) || e.type === 'hidden' || e.type === 'file') continue; /* a file input is hidden behind its label, which is the target */ const r = e.getBoundingClientRect(); const s = getComputedStyle(e);
    if (s.display === 'inline' && e.tagName === 'A') continue;
    if (e.classList.contains('stretch')) { const row = e.closest('.row'); if (row && row.getBoundingClientRect().height >= 44) continue; }
    if (r.height < 40 || r.width < 40) out.push(['small-target', `${label(e)} ${Math.round(r.width)}x${Math.round(r.height)}`]);
  }
  for (const e of document.querySelectorAll('main .btn, main .tag, .stat-value, .h-page')) { if (!vis(e)) continue; const lh = parseFloat(getComputedStyle(e).lineHeight) || parseFloat(getComputedStyle(e).fontSize) * 1.2; if (e.getBoundingClientRect().height > 70 && !e.classList.contains('h-page')) out.push(['wrapped', label(e)]); if (e.classList.contains('h-page') && e.getBoundingClientRect().height > lh * 3.2) out.push(['title-3-lines', label(e)]); }
  const fixed = [...document.querySelectorAll('.save-bar, .bottom-nav')].filter(vis).map(e => e.getBoundingClientRect());
  if (fixed.length === 2 && fixed[0].bottom > fixed[1].top + 1 && fixed[0].top < fixed[1].bottom) out.push(['overlap', 'save bar overlaps the bottom bar']);
  return out;
}
"""
found = collections.defaultdict(set)
errors = set()
with sync_playwright() as p:
    browser = p.chromium.launch()
    for width_name, viewport, who in (("phone", {"width": 390, "height": 844}, MANAGER),
                                      ("tablet", {"width": 820, "height": 1180}, FRONT_DESK),
                                      ("desktop", {"width": 1400, "height": 900}, OWNER)):
        context = browser.new_context(viewport=viewport)
        page = sign_in(context, who)
        page.on("console", lambda m: errors.add(m.text[:160]) if m.type == "error" else None)
        page.on("pageerror", lambda e: errors.add("PAGEERROR " + str(e)[:160]))
        for theme in ("daylight", "emerald"):
            context.add_cookies([{"name": "ncrm_theme", "value": theme, "url": URL}])
            for path in PATHS:
                page.goto(URL + path, wait_until="networkidle")
                page.wait_for_timeout(450)
                short = re.sub(r"[0-9a-f]{8}-[0-9a-f-]{27}", ":id", path)
                for kind, what in page.evaluate(CHECK):
                    found[(kind, short, what)].add(f"{width_name}/{theme}")
                if SHOTS:
                    name = short.strip("/").replace("/", "_").replace(":", "").replace("#", "-") or "home"
                    page.screenshot(path=str(OUT / f"{width_name}_{theme}_{name}.png"), full_page=True)
        context.close()
    # Signed out.
    context = browser.new_context(viewport={"width": 390, "height": 844})
    page = context.new_page()
    page.goto(URL + "/sign-in", wait_until="networkidle")
    for kind, what in page.evaluate(CHECK.replace("'main *, .sidebar *, .topbar *, .bottom-nav *'", "'main *'")):
        found[(kind, "/sign-in", what)].add("phone/daylight")
    if SHOTS: page.screenshot(path=str(OUT / "phone_daylight_sign-in.png"), full_page=True)
    browser.close()

by_kind = collections.defaultdict(list)
for (kind, path, what), where in found.items():
    by_kind[kind].append((path, what, sorted(where)))
for kind, rows in sorted(by_kind.items()):
    print(f"\n== {kind} ({len(rows)})")
    merged = collections.defaultdict(lambda: [set(), set()])
    for path, what, where in rows:
        merged[what][0].add(path); merged[what][1].update(where)
    for what, (paths, where) in sorted(merged.items(), key=lambda kv: -len(kv[1][0]))[:28]:
        w = sorted(where)
        print(f"  {what}  @ {len(paths)} screen(s) e.g. {sorted(paths)[0]}  [{'everywhere' if len(w) == 6 else ','.join(w)}]")
if not by_kind: print("Nothing measurable is wrong.")
print("\nconsole errors:", sorted(errors)[:6] or "none")
if SHOTS: print("screenshots:", OUT)
