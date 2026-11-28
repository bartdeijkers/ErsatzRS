#!/usr/bin/env python3
"""Run the channel-one playback gate against a new, isolated SQLite installation."""

import argparse
import contextlib
import datetime as dt
import hashlib
import importlib.metadata
import itertools
import json
import os
import shutil
import socket
import sqlite3
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

CALIBRATION = Path(__file__).resolve().parents[1] / "calibration"
sys.path.insert(0, str(CALIBRATION))

from channel1_fixture import generate
from channel1_av_offset import offset_limit

PROFILE = "1920x1080 x264 ac3 6ch"


def save(path, value):
    path.write_text(json.dumps(value, indent=2), encoding="utf-8")


def record_tools(out, tools, server, source):
    versions = {"python": sys.version.split()[0]}
    for package in ("numpy", "pillow", "python-vlc"):
        versions[package] = importlib.metadata.version(package)
    for name, command in (
        ("ffmpeg", [tools["ffmpeg"], "-version"]),
        ("ffprobe", [tools["ffprobe"], "-version"]),
        ("tsduck", ["tsp", "--version"]),
    ):
        versions[name] = subprocess.check_output(
            command, text=True, stderr=subprocess.STDOUT
        ).splitlines()[0]
    for name, path in (("server", server), ("scanner", tools["scanner"]), ("fixture", source)):
        with path.open("rb") as f:
            versions[name + "_sha256"] = hashlib.file_digest(f, "sha256").hexdigest()
    save(out / "versions.json", versions)


@contextlib.contextmanager
def host(out, server, port, ffmpeg, ffprobe):
    db = out / "state" / "gate.db"
    # The caller can override the port, but can never supply a DB or a base URL.
    with socket.socket() as sock:
        if os.name == "nt":
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_EXCLUSIVEADDRUSE, 1)
        sock.bind(("127.0.0.1", port))
        port = sock.getsockname()[1]
    addr = f"127.0.0.1:{port}"
    env = {
        k: v
        for k, v in os.environ.items()
        if not k.upper().startswith(
            ("ERSATZRS", "LEPTOS_", "DATABASE_", "JWT__", "FFREPORT", "CARGO_MANIFEST_DIR")
        )
    }
    env.update(
        DATABASE_URL="sqlite:" + db.as_posix(),
        ERSATZRS_DATABASE__URL="sqlite:" + db.as_posix(),
        ERSATZRS_APP_STATE_DIR=str(out / "state"),
        ERSATZRS_APP_DATA_DIR=str(out / "data"),
        ERSATZRS_DISABLE_HDHR_DISCOVERY="1",
        LEPTOS_SITE_ADDR=addr,
        LEPTOS_SITE_ROOT=str(server.parent / "site"),
        TEMP=str(out / "temp"),
        TMP=str(out / "temp"),
        TMPDIR=str(out / "temp"),
        ErsatzRS_FFMPEG__PATH=str(ffmpeg),
        ErsatzRS_FFPROBE__PATH=str(ffprobe),
        PATH=str(server.parent) + os.pathsep + env.get("PATH", ""),
    )
    base = "http://" + addr
    with (
        (out / "server.log").open("a") as log,
        subprocess.Popen(
            [server], env=env, cwd=out / "work", stdout=log, stderr=subprocess.STDOUT
        ) as p,
    ):
        try:
            for _ in range(180):
                if p.poll() is not None:
                    raise RuntimeError("isolated server exited; see server.log")
                try:
                    with urllib.request.urlopen(base + "/healthz", timeout=1) as response:
                        if response.status == 200 and db.is_file():
                            break
                except OSError:
                    pass
                time.sleep(0.5)
            else:
                raise RuntimeError("isolated server did not become healthy")
            yield base, db, env
        finally:
            if p.poll() is None:
                p.terminate()
                try:
                    p.wait(10)
                except subprocess.TimeoutExpired:
                    p.kill()


def prepare(out, db, env, tools, source):
    # Match the existing TSDuck gate's local library and cyclic playout seed.
    # Scan the real file so the ordinary probe owns all stream/page metadata.
    library = out / "library"
    library.mkdir()
    media = library / source.name
    try:
        os.link(source, media)
    except OSError:
        shutil.copyfile(source, media)
    with contextlib.closing(sqlite3.connect(db, timeout=25)) as c, c:
        c.row_factory = sqlite3.Row
        if c.execute("select count(*) from Playout").fetchone()[0]:
            raise RuntimeError("expected pristine playout database")
        mid = c.execute("insert into MediaSource default values").lastrowid
        c.execute("insert into LocalMediaSource(Id) values (?)", (mid,))
        lib = c.execute(
            "insert into Library(Name,MediaKind,MediaSourceId) values (?,1,?)", ("Calibration", mid)
        ).lastrowid
        c.execute("insert into LocalLibrary(Id) values (?)", (lib,))
        c.execute("insert into LibraryPath(Path,LibraryId) values (?,?)", (str(library), lib))
        profile = dict(c.execute("select * from FFmpegProfile where Name=?", (PROFILE,)).fetchone())
        channel = c.execute("select Id from Channel where Number='1'").fetchone()[0]
        c.execute(
            """update Channel set FFmpegProfileId=?, StreamingEngine=0, StreamingMode=5,
            IdleBehavior=0, IsEnabled=1, SubtitleMode=3, PreferredSubtitleLanguageCode='eng'
            where Id=?""",
            (profile["Id"], channel),
        )
        save(
            out / "profile.json",
            {
                "profile": profile,
                "channel": dict(
                    c.execute("select * from Channel where Id=?", (channel,)).fetchone()
                ),
            },
        )
    with (out / "scanner.log").open("w") as log:
        subprocess.run(
            [
                tools["scanner"],
                "--provider",
                "local",
                "--library-id",
                str(lib),
                "--deep-scan",
                "--ffprobe-path",
                tools["ffprobe"],
            ],
            cwd=out / "work",
            env=env,
            stdout=log,
            stderr=subprocess.STDOUT,
            check=True,
            timeout=180,
        )
    with contextlib.closing(sqlite3.connect(db, timeout=25)) as c, c:
        row = c.execute(
            "select v.MovieId from MediaVersion v join MediaFile f on f.MediaVersionId=v.Id where f.Path=?",
            (str(media),),
        ).fetchone()
        if not row:
            raise RuntimeError("scanner did not index the synthetic movie")
        collection = c.execute(
            "insert into Collection(Name,UseCustomPlaybackOrder) values ('Calibration',0)"
        ).lastrowid
        c.execute(
            "insert into CollectionItem(CollectionId,MediaItemId) values (?,?)",
            (collection, row[0]),
        )
        schedule = c.execute("""insert into ProgramSchedule(Name,FixedStartTimeBehavior,
            KeepMultiPartEpisodesTogether,RandomStartPoint,ShuffleScheduleItems,TreatCollectionsAsShows)
            values ('Calibration repeat',0,0,0,0,0)""").lastrowid
        item = c.execute(
            """insert into ProgramScheduleItem(ProgramScheduleId,"Index",CollectionType,
            CollectionId,FillWithGroupMode,GuideMode,MarathonGroupBy,MarathonShuffleGroups,
            MarathonShuffleItems,PlaybackOrder) values (?,0,0,?,0,0,0,0,0,1)""",
            (schedule, collection),
        ).lastrowid
        c.execute("insert into ProgramScheduleOneItem(Id) values (?)", (item,))
        c.execute(
            "insert into Playout(ChannelId,ScheduleKind,ProgramScheduleId,Seed) values (?,1,?,1)",
            (channel, schedule),
        )
        # Bootstrap's empty scheduler pass already ran. Make the newly seeded
        # schedule due on the next startup in this disposable database.
        c.execute("delete from ConfigElement where Key='maintenance.loop.playout.last_run'")


def wait_for_airings(out, db):
    # The host's startup scheduler builds the Classic timetable. Its UTC
    # boundaries can be anywhere relative to the invocation of this script.
    for _ in range(90):
        with contextlib.closing(sqlite3.connect(db)) as c:
            rows = c.execute("""select Start, Finish, MediaItemId from PlayoutItem
                where julianday(Start)>julianday('now','+18 seconds') order by Start limit 3""").fetchall()
        if len(rows) == 3:
            break
        time.sleep(1)
    else:
        raise RuntimeError("Classic scheduler did not publish the required airings")

    def utc(stamp):
        return dt.datetime.fromisoformat(stamp).replace(tzinfo=dt.UTC)

    starts = [utc(row[0]) for row in rows]
    if any(
        abs((utc(row[1]) - start).total_seconds() - 300) > 0.001 for row, start in zip(rows, starts)
    ) or any((b - a).total_seconds() != 300 for a, b in itertools.pairwise(starts)):
        raise RuntimeError("scheduler did not generate contiguous five-minute airings")
    save(out / "airings.json", rows)
    delay = (starts[0] - dt.datetime.now(dt.UTC)).total_seconds() - 18
    if delay > 301:
        raise RuntimeError("no scheduled calibration airing within one cycle")
    print(
        f"First complete airing starts at {starts[0].isoformat()}. Connecting in {max(0, delay):.0f} s.",
        flush=True,
    )
    time.sleep(max(0, delay))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", required=True, type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--fixture", type=Path, help="Previously generated channel-one clip")
    parser.add_argument("--ffmpeg", type=Path)
    parser.add_argument("--ffprobe", type=Path)
    parser.add_argument("--max-av-offset-ms", type=offset_limit,
                        help="Optional maximum absolute measured VLC audio/video offset")
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)

    def binary(name, explicit=None):
        sibling = server.with_name(name + server.suffix)
        return (
            explicit or (sibling if sibling.exists() else Path(shutil.which(name) or name))
        ).resolve(strict=True)

    result = {k: "NIET GEMETEN" for k in ("video", "audio", "teletext", "transport", "transition", "av_offset_measurement")}
    try:
        server = args.server.resolve(strict=True)
        tools = {
            "ffmpeg": binary("ffmpeg", args.ffmpeg),
            "ffprobe": binary("ffprobe", args.ffprobe),
            "scanner": server.with_name("ersatzrs-scanner" + server.suffix).resolve(strict=True),
        }
        from channel1_analysis import validate_source
        from channel1_metrics import evaluate
        from channel1_receiver import receive

        for name in ("state", "data", "temp", "work"):
            (out / name).mkdir()
        source = args.fixture
        if source is None:
            generate(str(tools["ffmpeg"]), out / "source")
            source = out / "source" / "Channel One (2026).ts"
        source = source.resolve(strict=True)
        record_tools(out, tools, server, source)
        validate_source(out, tools, source)
        print(f"Calibration source verified. Creating SQLite installation under {out}.", flush=True)
        with host(out, server, args.port, tools["ffmpeg"], tools["ffprobe"]) as (base, db, env):
            prepare(out, db, env, tools, source)
        with host(out, server, args.port, tools["ffmpeg"], tools["ffprobe"]) as (base, db, env):
            url = base + "/iptv/channel/1.ts"
            save(
                out / "run.json",
                {
                    "url": url,
                    "protocol": "HTTP unicast MPEG-TS",
                    "port": int(base.rsplit(":", 1)[1]),
                    "db": "state/gate.db",
                    "server": server.name,
                    "buffering_ms": 1000,
                },
            )
            wait_for_airings(out, db)
            print(f"Opening {url} in libVLC for two airings.", flush=True)
            data = receive(url, out)
        print("Playback ended. Analyzing received TS and presented video and audio.", flush=True)
        evaluate(out, tools, data, result, args.max_av_offset_ms)
    except Exception as exc:
        result["error"] = repr(exc)
        raise
    finally:
        result["total"] = (
            "PASS"
            if all(v == "PASS" for v in result.values())
            else ("FAIL" if "FAIL" in result.values() else "NIET GEMETEN")
        )
        save(out / "result.json", result)
        print(json.dumps(result), flush=True)
    return result["total"] != "PASS"


if __name__ == "__main__":
    sys.exit(main())
