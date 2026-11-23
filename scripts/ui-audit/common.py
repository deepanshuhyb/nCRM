"""Shared by audit.py and flows.py: where the app is, and how to sign in.

These scripts drive a real browser against the app that
`./scripts/dev.sh --fast <phone> "<name>"` runs, with the demo studio.
They are a developer's tool, not part of CI.

Needs:  pip install playwright && playwright install chromium
        psql on the PATH, and DATABASE_URL (read from ../../.env if unset)
        the dev script's output in a file, because sign-in codes are
        printed there:   ./scripts/dev.sh --fast ... 2>&1 | tee /tmp/ncrm-dev.log

Settings (environment variables):
  NCRM_URL      the dashboard, default http://localhost:3000
  NCRM_DEV_LOG  the file with the dev script's output, default /tmp/ncrm-dev.log
"""
import os, re, subprocess, time, pathlib

URL = os.environ.get("NCRM_URL", "http://localhost:3000")
DEV_LOG = os.environ.get("NCRM_DEV_LOG", "/tmp/ncrm-dev.log")
OUT = pathlib.Path(os.environ.get("NCRM_AUDIT_OUT", "/tmp/ncrm-audit"))
OUT.mkdir(parents=True, exist_ok=True)

# The demo studio's people (see crates/ncrm-api/src/bin/ncrm-dev-seed.rs).
MANAGER, FRONT_DESK, INSTRUCTOR = "6000000001", "6000000002", "6000000003"


def database_url():
    if os.environ.get("DATABASE_URL"):
        return os.environ["DATABASE_URL"]
    env = pathlib.Path(__file__).resolve().parents[2] / ".env"
    for line in env.read_text().splitlines():
        if line.startswith("DATABASE_URL="):
            return line.split("=", 1)[1].strip().strip('"')
    raise SystemExit("Set DATABASE_URL, or run from a checkout with a .env")


def sql(query):
    """One value from the dev database. Only used to look up ids and to
    clear the five-codes-an-hour sign-in limit between runs."""
    done = subprocess.run(["psql", database_url(), "-Atc", query], capture_output=True, text=True)
    return done.stdout.strip()


def allow_more_codes():
    sql("DELETE FROM platform.login_codes")


def sign_in(context, phone):
    """Sign in as a demo person. The code is read from the dev script's
    output, which is where the local-only `Log` code sender prints it."""
    page = context.new_page()
    page.goto(URL + "/sign-in", wait_until="networkidle")
    page.locator("input").first.fill(phone)
    page.get_by_role("button").first.click()
    time.sleep(2)
    codes = re.findall(r"sign-in code for \+91%s: (\d+)" % phone, open(DEV_LOG).read())
    if not codes:
        raise SystemExit(f"No sign-in code for {phone} in {DEV_LOG}. Is NCRM_DEV_LOG right?")
    page.locator("input").last.fill(codes[-1])
    button = page.get_by_role("button", name=re.compile("Sign in|Verify|Continue", re.I))
    if button.count():
        button.first.click()
    page.wait_for_url(URL + "/", timeout=30000)
    return page
