"""Walk the main jobs on a phone-sized screen as the demo manager and say which steps work:
add a student (the empty form must complain first), put them in a class, take a
payment and open the receipt, switch profile tabs, mark a class, add a class,
search, add a room, open a team editor, answer a parent in the WhatsApp inbox,
switch the bot off and on, message a class, switch fee reminders off and on, switch theme, sign out.

usage:  python3 scripts/ui-audit/flows.py

It ADDS a test student, a class, a room and a payment to the demo studio every
time it runs. See common.py for what it needs.
"""
import re, time
from playwright.sync_api import sync_playwright, expect
from common import URL as B, OUT, MANAGER, allow_more_codes, database_url, sign_in

allow_more_codes()
results = []
def step(name, fn):
    try:
        out = fn(); results.append(("ok", name, out or "")); print("ok  ", name, "|", out or "")
    except Exception as e:
        results.append(("FAIL", name, str(e).split("\n")[0][:200])); print("FAIL", name, "|", str(e).split("\n")[0][:200])
        try: pg.screenshot(path=str(OUT / f"flow_FAIL_{len(results)}.png"), full_page=True)
        except Exception: pass
with sync_playwright() as p:
    b = p.chromium.launch()
    ctx = b.new_context(viewport={"width": 390, "height": 844}, has_touch=True)
    pg = sign_in(ctx, MANAGER)
    errs = []
    pg.on("console", lambda m: errs.append(m.text[:200]) if m.type == "error" else None)
    pg.on("pageerror", lambda e: errs.append("PAGEERROR " + str(e)[:200]))
    shot = lambda n: pg.screenshot(path=str(OUT / f"flow_{n}.png"), full_page=True)
    name = f"Test Student {int(time.time()) % 100000}"

    def add_student():
        pg.goto(B + "/students", wait_until="networkidle")
        pg.get_by_role("link", name="Add student").click(); pg.wait_for_url(re.compile("/students/new"))
        pg.get_by_role("button", name="Add student").click(); pg.wait_for_timeout(400); shot("student_errors")
        pg.get_by_label("Full name").fill(name)
        pg.get_by_label("Contact name").fill("Test Parent")
        pg.get_by_label("WhatsApp number").fill("98111 " + str(int(time.time()) % 100000).zfill(5))
        pg.get_by_role("button", name="Add student").click()
        pg.wait_for_url(re.compile(r"/students/[0-9a-f-]{36}"), timeout=20000)
        return pg.locator("h1").inner_text()
    step("add a student (empty form shows errors, then saves)", add_student)
    profile = pg.url.split("#")[0]

    def join_class():
        pg.get_by_role("button", name="Put in a class").click(); pg.wait_for_timeout(300); shot("put_in_class")
        # The first class with room: a full class is listed but can't be chosen.
        klass = pg.get_by_label("Class", exact=True)
        klass.select_option(value=klass.locator("option:not([disabled]):not([value=''])").first.get_attribute("value"))
        pay = pg.get_by_label("How they pay")
        opts = pay.locator("option").all_inner_texts(); pay.select_option(index=1 if len(opts) > 1 else 0)
        pg.get_by_role("button", name="Add to class").click()
        pg.get_by_role("button", name=re.compile("^Take .* out of the class")).first.wait_for(timeout=15000)
        return f"paying: {opts[1 if len(opts) > 1 else 0]}"
    step("put them in a class", join_class)

    def pay():
        pg.get_by_role("tab", name=re.compile("Fees")).click(); pg.wait_for_timeout(500); shot("fees_tab")
        btn = pg.get_by_role("button", name=re.compile("^Record"))
        if not btn.count():
            pg.locator("summary", has_text=re.compile("payment", re.I)).first.click(); pg.wait_for_timeout(300)
        amount = pg.get_by_label(re.compile("Amount received"))
        if not amount.input_value(): amount.fill("500")
        pg.get_by_role("button", name=re.compile("^Record")).click()
        link = pg.get_by_role("link", name=re.compile("^Receipt")).first; link.wait_for(timeout=15000)
        assert pg.get_by_role("tab", selected=True).inner_text().strip().startswith("Fees"), "left the Fees tab after saving"
        link.click(); pg.wait_for_url(re.compile("/fees/receipts/")); shot("receipt")
        return pg.locator(".receipt").inner_text().replace("\n", " ")[:90]
    step("take a payment and open the receipt", pay)

    def tabs():
        pg.goto(profile + "#details", wait_until="networkidle"); pg.wait_for_timeout(400); shot("details_tab")
        sel = pg.get_by_role("tab", selected=True).inner_text()
        pg.get_by_role("tab", name=re.compile("Attendance")).click(); pg.wait_for_timeout(300); shot("attendance_tab")
        return f"deep link opened: {sel.strip()}"
    step("profile tabs and deep link", tabs)

    def mark():
        pg.goto(B + "/attendance", wait_until="networkidle")
        pg.locator(".day-strip a, a[href^='/attendance/']").last.click()
        pg.wait_for_url(re.compile("/attendance/[0-9a-f-]{36}"))
        everyone = pg.get_by_role("button", name="Everyone's here")
        if not everyone.count(): return "already marked on an earlier run; skipped"
        everyone.click()
        pg.get_by_role("group", name=re.compile("^Mark ")).nth(1).get_by_role("button", name="Absent").click()
        pg.wait_for_timeout(200); shot("marking")
        pg.get_by_role("button", name="Save attendance").click()
        expect(pg.get_by_text("Saved.")).to_be_visible(timeout=15000)
        pg.get_by_role("button", name="Teacher was here").click(); pg.wait_for_timeout(1200)
        pressed = pg.get_by_role("button", name="Teacher was here").get_attribute("aria-pressed")
        shot("marked")
        return pg.locator("#roster-title").inner_text() + f" | teacher pressed={pressed}"
    step("mark a class and the teacher", mark)

    def new_class():
        pg.goto(B + "/classes/new", wait_until="networkidle")
        pg.get_by_label("Name", exact=True).fill(f"Test Class {int(time.time()) % 1000}")
        pg.get_by_label("Branch", exact=True).select_option(index=1); pg.wait_for_timeout(300)
        # No room: a room can't hold two classes at once, and this script runs more than once.
        pg.locator("label.day-toggle", has_text="Sat").click()
        pg.wait_for_timeout(200); shot("new_class")
        pg.get_by_role("button", name="Add class").click()
        pg.wait_for_url(re.compile(r"/classes/[0-9a-f-]{36}"), timeout=20000)
        return pg.locator("h1").inner_text().replace("\n", " ")
    step("add a class", new_class)

    def search():
        pg.goto(B + "/students", wait_until="networkidle")
        pg.get_by_placeholder("Name or phone number").fill("Test Student"); pg.get_by_role("button", name="Search").click()
        pg.wait_for_url(re.compile("q=")); pg.wait_for_timeout(500); shot("search")
        return pg.locator("main").get_by_text(re.compile(r"\d+ (student|match)")).first.inner_text()
    step("search students", search)

    def room():
        pg.goto(B + "/branches", wait_until="networkidle")
        pg.locator("summary", has_text="Add a room").first.click(); pg.wait_for_timeout(300); shot("add_room")
        pg.get_by_label("Room name").first.fill(f"Room {int(time.time()) % 1000}")
        pg.get_by_role("button", name="Add room").first.click(); pg.wait_for_timeout(1500)
        return "added" if pg.get_by_text(re.compile(r"^Room \d+$")).count() else "not shown"
    step("add a room", room)

    def team():
        pg.goto(B + "/team", wait_until="networkidle")
        pg.locator("summary").first.click(); pg.wait_for_timeout(300); shot("team_edit")
        return f"{pg.locator('summary').count()} editable people"
    step("open a team member's editor", team)

    def inbox():
        # A parent writes (through the same code the webhook uses), the bot
        # doesn't understand, and the manager answers from the inbox.
        import subprocess, pathlib
        root = pathlib.Path(__file__).resolve().parents[2]
        said = f"Is the studio open on the 26th? ({int(time.time()) % 100000})"
        done = subprocess.run([str(root / "target/debug/ncrm-admin"), "whatsapp", "say", "rhythm-house", "60001 00003", said],
                              capture_output=True, text=True, env={**__import__("os").environ, "DATABASE_URL": database_url()})
        assert done.returncode == 0, done.stderr[-200:]
        pg.goto(B + "/inbox", wait_until="networkidle")
        row = pg.locator("a.row", has_text="Needs you").first
        row.click(); pg.wait_for_url(re.compile(r"/inbox/[0-9a-f-]{36}"))
        expect(pg.get_by_text(said)).to_be_visible(timeout=10000)
        pg.get_by_label(re.compile("^Reply to")).fill("Yes, we're open as usual.")
        pg.get_by_role("button", name="Send on WhatsApp").click()
        expect(pg.locator(".bubble[data-from='us']", has_text="Yes, we're open as usual.").last).to_be_visible(timeout=15000)
        pg.wait_for_timeout(1500); pg.reload(wait_until="networkidle"); shot("inbox_thread")
        status = pg.locator(".bubble[data-from='us'] .bubble-meta").last.inner_text()
        pg.get_by_role("button", name="Hand back to the bot").click(); pg.wait_for_timeout(1200)
        gone = pg.get_by_role("button", name="Hand back to the bot").count() == 0
        return f"reply shown: {status.strip()} | handed back: {gone}"
    step("answer a parent in the inbox", inbox)

    def bot_mode():
        pg.goto(B + "/inbox", wait_until="networkidle")
        pg.locator("summary", has_text="The bot").click()
        pg.get_by_role("button", name="Off", exact=True).click(); pg.wait_for_timeout(1200)
        pg.reload(wait_until="networkidle")
        off = pg.locator("summary", has_text="The bot").inner_text().replace("\n", " ")
        pg.locator("summary", has_text="The bot").click()
        pg.get_by_role("button", name="Menu", exact=True).click(); pg.wait_for_timeout(1200); shot("inbox")
        return f"after switching off: {off.strip()}"
    step("switch the bot off and back to the menu", bot_mode)

    def group_message():
        pg.goto(B + "/inbox", wait_until="networkidle")
        pg.get_by_role("link", name="Message a group").click(); pg.wait_for_url(re.compile("/inbox/broadcast"))
        # Nothing written yet: it says so and doesn't check.
        pg.get_by_role("button", name="Check who gets it").click()
        expect(pg.get_by_text("Write the message first.")).to_be_visible()
        said = f"No class tomorrow, the hall is being painted. ({int(time.time()) % 100000})"
        pg.get_by_label("Message", exact=True).fill(said)
        pg.get_by_role("button", name="Class", exact=True).click()
        pg.get_by_label("Class", exact=True).select_option(index=1)
        pg.get_by_role("button", name="Check who gets it").click()
        send = pg.get_by_role("button", name=re.compile(r"^Send to \d+ (person|people)$")); send.wait_for(timeout=15000)
        shot("broadcast_check")
        label = send.inner_text()
        send.click()
        expect(pg.get_by_text(re.compile(r"^Sent to \d+ (person|people)\."))).to_be_visible(timeout=15000)
        expect(pg.locator("#sent-title").locator("xpath=ancestor::section").get_by_text(said)).to_be_visible(timeout=15000)
        pg.wait_for_timeout(1500); pg.reload(wait_until="networkidle"); shot("broadcast_sent")
        counts = pg.locator("section[aria-labelledby='sent-title'] li").first.locator("p").nth(2).inner_text()
        return f"{label} | {counts.strip()}"
    step("message a class, after checking who gets it", group_message)

    def reminders_switch():
        pg.goto(B + "/fees", wait_until="networkidle")
        box = pg.locator("details", has_text="WhatsApp reminders")
        before = box.locator("summary").inner_text().replace("\n", " ")
        box.locator("summary").click()
        box.get_by_role("button", name=re.compile("^Turn reminders (off|on)$")).click(); pg.wait_for_timeout(1500)
        pg.reload(wait_until="networkidle")
        box = pg.locator("details", has_text="WhatsApp reminders")
        after = box.locator("summary").inner_text().replace("\n", " ")
        box.locator("summary").click(); shot("fee_reminders")
        box.get_by_role("button", name=re.compile("^Turn reminders (off|on)$")).click(); pg.wait_for_timeout(1200)
        return f"{before.strip()} -> {after.strip()} -> back"
    step("switch fee reminders off and on again", reminders_switch)

    def theme():
        pg.goto(B + "/", wait_until="networkidle")
        before = pg.evaluate("document.documentElement.dataset.theme || ''")
        pg.locator(".topbar button").first.click(); pg.wait_for_timeout(500)
        after = pg.evaluate("document.documentElement.dataset.theme || ''"); shot("home_other_theme")
        pg.locator(".topbar button").first.click(); pg.wait_for_timeout(300)
        return f"{before} -> {after}"
    step("switch theme", theme)

    def more_and_out():
        pg.locator(".bottom-nav a", has_text="More").click(); pg.wait_for_url(re.compile("/more")); shot("more")
        pg.get_by_role("button", name="Sign out").click(); pg.wait_for_url(re.compile("/sign-in"), timeout=15000)
        return "signed out"
    step("More, then sign out", more_and_out)
    print("console errors:", errs[:6])
    b.close()
print(sum(1 for r in results if r[0] == "ok"), "of", len(results), "steps passed")
