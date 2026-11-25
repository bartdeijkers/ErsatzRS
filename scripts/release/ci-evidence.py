"""Read the completed-check provenance recorded by full CI for a release."""

import json
import re
import subprocess


NAMES = {
    "clippy": "Clippy",
    "test": "Test",
    "next_engine": "Next engine",
    "teletext": "Teletext 1.5",
}
BROWSERS = {(suite, project) for suite in ("general", "1.5")
            for project in ("chromium", "firefox")}


def composed_checks(log, source):
    """Return the checks the exact-source aggregate accepted from earlier CI."""
    plain = re.sub(r"\x1b\[[0-9;]*[A-Za-z]", "", log)
    plain = re.sub(r"^\ufeff?\d{4}-\d\d-\d\dT\S+ ", "", plain, flags=re.M)
    records = re.findall(r"^Verified reused check provenance: (.+)$", plain, re.M)
    if len(records) != 1 or "All gate jobs passed (full lane)." not in plain.splitlines():
        return set()
    try:
        proof = json.loads(records[0])
        if proof["execution_sha"] != source or not re.fullmatch(r"[a-f0-9]{40}", proof["source_sha"]):
            return set()
        earlier_jobs = proof["jobs"]
        if set(earlier_jobs) - (set(NAMES) | {"hydration"}):
            return set()
        pending = {(row["suite"], row["project"]) for row in proof["browser_pending"]}
        if (len(pending) != len(proof["browser_pending"]) or not pending <= BROWSERS
                or len(proof["browser_passed"]) != len(BROWSERS - pending)
                or ("hydration" in earlier_jobs) != (not pending)):
            return set()
        return {NAMES[job] for job in earlier_jobs if job in NAMES} | {
            f"Playwright E2E ({suite}, {project})" for suite, project in BROWSERS - pending
        }
    except (KeyError, ValueError, TypeError):
        return set()


def completed_checks(repo, source, jobs):
    passed = {row["name"] for row in jobs
              if row["status"] == "completed" and row["conclusion"] == "success"}
    if not {"Verify reused CI artifacts", "CI ok"} <= passed:
        return passed
    aggregates = [row for row in jobs if row["name"] == "CI ok"]
    if len(aggregates) != 1 or not (job_id := aggregates[0].get("databaseId")):
        return passed
    cmd = ["gh", "api", f"repos/{repo}/actions/jobs/{job_id}/logs"]
    if "--allow-escape-sequences" in subprocess.check_output(["gh", "api", "--help"], text=True):
        cmd.append("--allow-escape-sequences")
    try:
        log = subprocess.check_output(cmd, text=True)
    except subprocess.CalledProcessError:
        return passed
    return passed | composed_checks(log, source)
